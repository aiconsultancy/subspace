import { and, eq } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { defaultDevEmail, defaultDevName, defaultDevOrg, DevCredentials } from "../src/api";
import { mintDevToken, readDevKey } from "../src/auth";
import { config } from "../src/config";
import { Database, databaseLayer } from "../src/db";
import { localMember } from "../src/identity";
import { organisations, users } from "../src/schema";

const options = config();
if (!options.dev)
  throw new Error("Set AUTH_PROVIDERS=dev and ENABLE_DEV_AUTH=true to mint a local token.");

const credentials = Schema.decodeUnknownSync(DevCredentials)({
  email: process.argv[2] ?? defaultDevEmail,
  name: process.argv[4] ?? defaultDevName,
  organisation: process.argv[3] ?? defaultDevOrg,
});
const email = credentials.email!.toLowerCase();
const org = credentials.organisation!;
const key = await readDevKey(options.dev.keyPath);

// The fleet installer sets this for a database it did not create: sign in only as a member who is already
// there, so a mistyped organisation or email never adds an organisation or a person. A database with no
// organisation at all is a new tracker, and its first sign-in creates one as usual.
if (process.env.DEV_TOKEN_EXISTING_ONLY === "true") {
  const known = Effect.gen(function* () {
    const db = yield* Database;
    const [anyOrganisation] = yield* db
      .select({ id: organisations.id })
      .from(organisations)
      .limit(1);
    if (!anyOrganisation) return true;
    const [row] = yield* db
      .select({ id: users.id })
      .from(users)
      .innerJoin(organisations, eq(users.organisationId, organisations.id))
      .where(
        and(
          eq(organisations.provider, "dev"),
          eq(organisations.externalId, org),
          eq(users.subject, email),
        ),
      );
    return row !== undefined;
  });
  if (!(await Effect.runPromise(known.pipe(Effect.provide(databaseLayer(options.databaseUrl)))))) {
    console.error(
      `The database has no member ${email} in development organisation ${org}, and DEV_TOKEN_EXISTING_ONLY=true forbids adding one.`,
    );
    process.exit(4);
  }
}

const member = await Effect.runPromise(
  localMember(email, credentials.name!, org, options.dev, options.reservedUsernames, {
    name: process.argv[4],
    email,
  }).pipe(Effect.provide(databaseLayer(options.databaseUrl))),
);
if (!member.active) throw new Error("This membership is inactive.");
// Output is intentionally machine-readable for the caller. Redirect it into a
// private variable or store if you do not want it to appear in your terminal.
console.log(await mintDevToken(key, email, org));
