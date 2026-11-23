import { Effect, Schema } from "effect";
import { Database, databaseLayer, databaseUrl } from "../src/db";
import { setUsername, UsernameRefused } from "../src/usernames";
import { Uuid } from "../src/validation";

/**
 * Sets a person's username explicitly, so that the owner of an existing IRC account is given its
 * name: bun run auth:username workos|entra|dev ORG SUBJECT NAME. A reserved name is allowed; a
 * name another member or agent holds or held is refused. The change is audited.
 */
const [provider, organisation, subject, name, ...extra] = process.argv.slice(2);
if (
  !["workos", "entra", "dev"].includes(provider ?? "") ||
  !organisation ||
  !subject ||
  !name ||
  extra.length ||
  organisation.length > 256 ||
  subject.length > 256
) {
  throw new Error("Usage: bun run auth:username workos|entra|dev ORG SUBJECT NAME");
}

const id = Schema.decodeUnknownSync(Uuid);
const identity =
  provider === "entra"
    ? {
        provider: "entra" as const,
        externalOrganisation: id(organisation).toLowerCase(),
        subject: id(subject).toLowerCase(),
      }
    : {
        provider: provider as "workos" | "dev",
        externalOrganisation: organisation,
        subject: provider === "dev" ? subject.toLowerCase() : subject,
      };

try {
  const result = await Effect.runPromise(
    Effect.flatMap(Database, (db) => setUsername(db, identity, name)).pipe(
      Effect.provide(databaseLayer(databaseUrl(), 2)),
    ),
  );
  console.log(
    result.previous
      ? `Username changed from ${result.previous} to ${result.username}; ${result.previous} is retired.`
      : `Username set to ${result.username}.`,
  );
} catch (error) {
  const refusal = error instanceof UsernameRefused ? error : (error as { cause?: unknown }).cause;
  console.error(
    refusal instanceof UsernameRefused
      ? refusal.message
      : "The username could not be saved. Check the database configuration and migrations.",
  );
  process.exitCode = 1;
}
