import { and, asc, eq, inArray, isNull, sql } from "drizzle-orm";
import { Context, Effect } from "effect";
import { type Database } from "./db";
import { organisations, users } from "./schema";

/**
 * Usernames: one handle per person, which is also their IRC account name. People and agents
 * share one namespace per organisation, compared without regard to case; an agent's username is
 * its IRC nick. An allocated username is lower-case ASCII letters and digits, starts with a
 * letter and has at most 32 characters, which Ergo accepts as an account name.
 */
type Db = (typeof Database)["Service"];

export const maxUsernameLength = 32;
export const allocatedUsername = /^[a-z][a-z0-9]{0,31}$/u;

/** Ergo's service names and its guest nick, which no person or agent may take. */
export const builtInReservedUsernames: ReadonlySet<string> = new Set([
  "nickserv",
  "chanserv",
  "operserv",
  "hostserv",
  "histserv",
  "botserv",
  "memoserv",
  "global",
  "guest",
]);

/** The operator's reserved names from RESERVED_USERNAMES, in lower case. */
export const ReservedUsernames = Context.Reference<ReadonlySet<string>>(
  "subspace/ReservedUsernames",
  { defaultValue: () => new Set() },
);

/** Whether a name is reserved: an IRC service, `guest`, any `guest-` name or an operator name. */
export function isReservedUsername(name: string, reserved: ReadonlySet<string>) {
  const lower = name.toLowerCase();
  return builtInReservedUsernames.has(lower) || lower.startsWith("guest-") || reserved.has(lower);
}

/** Letters that do not decompose into an ASCII letter and a diacritic. */
const foldedLetters: Readonly<Record<string, string>> = {
  ß: "ss",
  æ: "ae",
  œ: "oe",
  ø: "o",
  đ: "d",
  ð: "d",
  þ: "th",
  ł: "l",
  ı: "i",
  ħ: "h",
  ŧ: "t",
  ŋ: "n",
  ĸ: "k",
  ŀ: "l",
  ſ: "s",
};

/** The lower-case ASCII letters and digits of a name, with diacritics folded and the rest dropped. */
export function foldName(value: string) {
  return value
    .toLowerCase()
    .normalize("NFKD")
    .replace(/\p{M}/gu, "")
    .replace(/[^a-z0-9]/gu, (character) => foldedLetters[character] ?? "");
}

/** A folded first name from its first letter, at most 32 characters. */
const firstName = (value: string) =>
  foldName(value)
    .replace(/^[0-9]+/u, "")
    .slice(0, maxUsernameLength);

/** The first word that folds to a first name, and the words after it as the surname. */
function fromWords(words: readonly string[], surname?: string) {
  const index = words.findIndex((word) => firstName(word) !== "");
  if (index < 0) return undefined;
  return {
    first: firstName(words[index]!),
    surname: foldName(surname ?? words.slice(index + 1).join("")),
  };
}

/** The name words of `name`. "Lovelace, Ada" names the surname first. */
function splitName(name: string) {
  const comma = name.indexOf(",");
  if (comma < 0) return fromWords(name.split(/\s+/u));
  return fromWords(name.slice(comma + 1).split(/\s+/u), name.slice(0, comma));
}

/** The words of an e-mail local part, such as `ada.lovelace+tag`. */
function splitEmail(email: string) {
  const local = email.slice(0, Math.max(email.lastIndexOf("@"), 0)).split("+", 1)[0]!;
  return fromWords(local.split(/[._-]+/u));
}

/** Where a person's username comes from. Only the profile; never their identity. */
export interface NameSource {
  readonly givenName?: string | null;
  readonly familyName?: string | null;
  readonly name?: string | null;
  readonly email?: string | null;
}

/**
 * The first name and surname a username is made from: the given and family names, else the
 * words of the display name, else the e-mail local part read as `first.last`, else `user`.
 */
export function nameParts(source: NameSource) {
  const fromName = source.name ? splitName(source.name) : undefined;
  const given = firstName(source.givenName ?? "");
  if (given) {
    return { first: given, surname: foldName(source.familyName ?? "") || fromName?.surname || "" };
  }
  return (
    fromName ??
    (source.email ? splitEmail(source.email) : undefined) ?? { first: "user", surname: "" }
  );
}

/**
 * Usernames to try, in order: the first name, then the first name with successive letters of the
 * surname (`ada`, `adal`, `adalo`), then the first name with a number from 2 (`ada2`), all within
 * 32 characters.
 */
export function* usernameCandidates(parts: { first: string; surname: string }) {
  const { first, surname } = parts;
  yield first;
  for (let length = 1; length <= surname.length; length++) {
    if (first.length + length > maxUsernameLength) break;
    yield first + surname.slice(0, length);
  }
  for (let number = 2; ; number++) {
    const suffix = String(number);
    yield first.slice(0, maxUsernameLength - suffix.length) + suffix;
  }
}

/**
 * Serialises username allocation in an organisation, with agent registration changes, which
 * take the same lock, until the transaction ends.
 */
export const lockUsernames = (db: Db, organisationId: string) =>
  db
    .select({ id: organisations.id })
    .from(organisations)
    .where(eq(organisations.id, organisationId))
    .for("no key update");

/**
 * The first free, unreserved candidate username for a person in an organisation. Call it inside
 * a transaction that holds `lockUsernames`; the unique index is the final guarantee.
 */
export const allocateUsername = (
  db: Db,
  organisationId: string,
  source: NameSource,
  reserved: ReadonlySet<string>,
) =>
  Effect.gen(function* () {
    const candidates = usernameCandidates(nameParts(source));
    for (;;) {
      const batch: string[] = [];
      while (batch.length < 64) {
        const candidate = candidates.next().value!;
        if (!isReservedUsername(candidate, reserved)) batch.push(candidate);
      }
      const taken = new Set(
        (yield* db
          .select({ name: sql<string>`lower(${users.username})` })
          .from(users)
          .where(
            and(
              eq(users.organisationId, organisationId),
              inArray(sql`lower(${users.username})`, batch),
            ),
          )).map((row) => row.name),
      );
      const free = batch.find((candidate) => !taken.has(candidate));
      if (free) return free;
    }
  });

/**
 * Gives every person without a username one, oldest first, by the same rule as registration.
 * Agents already have theirs from their IRC nicks. It is deterministic for a given database and
 * changes nothing when every person has a username, so running it again is safe.
 */
export const backfillUsernames = (db: Db, reserved: ReadonlySet<string>) =>
  Effect.gen(function* () {
    const people = yield* db
      .select({
        id: users.id,
        organisationId: users.organisationId,
        displayName: users.displayName,
        email: users.email,
      })
      .from(users)
      .where(and(isNull(users.username), isNull(users.agentRank)))
      .orderBy(asc(users.createdAt), asc(users.id));
    const locked = new Set<string>();
    for (const person of people) {
      if (!locked.has(person.organisationId)) {
        yield* lockUsernames(db, person.organisationId);
        locked.add(person.organisationId);
      }
      const username = yield* allocateUsername(
        db,
        person.organisationId,
        { name: person.displayName, email: person.email },
        reserved,
      );
      yield* db
        .update(users)
        .set({ username })
        .where(and(eq(users.id, person.id), isNull(users.username)));
    }
    return people.length;
  });
