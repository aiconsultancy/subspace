import { parseArgs } from "node:util";
import { Effect, Schema } from "effect";
import { Database, databaseLayer, databaseUrl } from "../src/db";
import { releaseUsername, setUsername, UsernameRefused } from "../src/usernames";
import { Uuid } from "../src/validation";

/**
 * The username command, for administrators. Every change is audited.
 *
 *   bun run auth:username [--retired] workos|entra|dev ORG SUBJECT NAME
 *     Sets a member's username, or an agent's IRC nick, which is its username: so that the owner
 *     of an existing IRC account is given its name, or an agent registered again gets its old
 *     nick back. A reserved name is allowed; a name another member or agent holds is refused,
 *     and so is a retired one unless --retired hands it to this member or agent.
 *
 *   bun run auth:username --release workos|entra|dev ORG NAME
 *     Releases a retired name, so that it may be allocated or given again.
 */
const usage =
  "Usage: bun run auth:username [--retired] workos|entra|dev ORG SUBJECT NAME\n" +
  "       bun run auth:username --release workos|entra|dev ORG NAME";
const { values, positionals } = parseArgs({
  args: process.argv.slice(2),
  allowPositionals: true,
  options: { retired: { type: "boolean" }, release: { type: "boolean" } },
});
const [provider, organisation, ...rest] = positionals;
const expected = values.release ? 1 : 2;
if (
  !["workos", "entra", "dev"].includes(provider ?? "") ||
  !organisation ||
  rest.length !== expected ||
  rest.some((value) => !value || value.length > 256) ||
  organisation.length > 256 ||
  (values.release && values.retired)
) {
  throw new Error(usage);
}

const id = Schema.decodeUnknownSync(Uuid);
const organisationRef =
  provider === "entra"
    ? { provider: "entra" as const, externalOrganisation: id(organisation).toLowerCase() }
    : { provider: provider as "workos" | "dev", externalOrganisation: organisation };
const subjectOf = (subject: string) =>
  provider === "entra"
    ? id(subject).toLowerCase()
    : provider === "dev"
      ? subject.toLowerCase()
      : subject;

const work = values.release
  ? Effect.flatMap(Database, (db) => releaseUsername(db, organisationRef, rest[0]!)).pipe(
      Effect.map((result) => `Released ${result.name}; it may be allocated or given again.`),
    )
  : Effect.flatMap(Database, (db) =>
      setUsername(db, { ...organisationRef, subject: subjectOf(rest[0]!) }, rest[1]!, {
        reuseRetired: values.retired === true,
      }),
    ).pipe(
      Effect.map((result) =>
        result.previous
          ? `Username changed from ${result.previous} to ${result.username}; ${result.previous} is retired.`
          : `Username set to ${result.username}.`,
      ),
    );

try {
  console.log(await Effect.runPromise(work.pipe(Effect.provide(databaseLayer(databaseUrl(), 2)))));
} catch (error) {
  const refusal = error instanceof UsernameRefused ? error : (error as { cause?: unknown }).cause;
  console.error(
    refusal instanceof UsernameRefused
      ? refusal.message
      : "The username could not be saved. Check the database configuration and migrations.",
  );
  process.exitCode = 1;
}
