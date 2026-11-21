import { and, eq, inArray } from "drizzle-orm";
import { Effect } from "effect";
import { marked, type Token } from "marked";
import { type Database } from "./db";
import { memberKind, type Member } from "./identity";
import { problem } from "./problems";
import { fleets, ships, users } from "./schema";
import { isUuid } from "./validation";

export function markdownMentions(body: string | null | undefined) {
  const mentions = new Map<string, { userId: string; label: string }>();
  function collect(tokens: Token[]) {
    for (let index = 1; index < tokens.length; index++) {
      const current = tokens[index]!;
      const previous = tokens[index - 1]!;
      if (current.type !== "link" || !current.raw.startsWith("[") || !previous.raw.endsWith("@"))
        continue;
      const id = current.href.startsWith("member:") ? current.href.slice(7) : "";
      if (isUuid(id)) {
        const userId = id.toLowerCase();
        if (!mentions.has(userId) && mentions.size < 100)
          mentions.set(userId, { userId, label: String(current.text).slice(0, 120) });
      }
    }
  }
  if (body)
    marked.walkTokens(marked.lexer(body), (token) => {
      if ("tokens" in token && Array.isArray(token.tokens)) collect(token.tokens);
    });
  return [...mentions.values()];
}

export function newMentionIds(body: string | null | undefined, old?: string | null) {
  const prior = new Set(markdownMentions(old).map(({ userId }) => userId));
  return markdownMentions(body)
    .map(({ userId }) => userId)
    .filter((id) => !prior.has(id));
}

type Db = (typeof Database)["Service"];
export function mentionStatuses(body: string | null, people: ReadonlyMap<string, Member>) {
  return markdownMentions(body).map(({ userId }) => {
    const person = people.get(userId);
    return {
      userId,
      status: person?.active ? ("canRead" as const) : ("notMember" as const),
      displayName: person?.active ? person.displayName : null,
      role: person?.active ? memberKind(person) : null,
      reason: null,
    };
  });
}

export function mentionPeople(db: Db, member: Member, ids: string[]) {
  return ids.length
    ? Effect.map(
        db
          .select({ member: users, fleet: fleets.slug, ship: ships.name })
          .from(users)
          .leftJoin(fleets, eq(users.agentFleetId, fleets.id))
          .leftJoin(ships, eq(users.agentShipId, ships.id))
          .where(and(eq(users.organisationId, member.organisationId), inArray(users.id, ids))),
        (rows) =>
          rows.map(({ member, fleet, ship }) => ({
            ...member,
            agentFleet: fleet ?? member.agentFleet,
            agentShip: ship ?? member.agentShip,
          })),
      )
    : Effect.succeed([] as Member[]);
}

export function validatedMentions(
  db: Db,
  member: Member,
  body?: string | null,
  old?: string | null,
) {
  return Effect.gen(function* () {
    const ids = newMentionIds(body, old);
    const people = yield* mentionPeople(db, member, ids);
    if (ids.length !== people.filter((person) => person.active).length)
      return yield* Effect.fail(
        problem(400, "Mention existing active members of this organisation."),
      );
    return ids;
  });
}
