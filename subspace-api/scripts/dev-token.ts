import { Effect, Schema } from "effect";
import { defaultDevEmail, defaultDevName, defaultDevOrg, DevCredentials } from "../src/api";
import { mintDevToken, readDevKey } from "../src/auth";
import { config } from "../src/config";
import { databaseLayer } from "../src/db";
import { localMember } from "../src/identity";

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
