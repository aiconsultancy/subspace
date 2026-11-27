/**
 * Creates a pairing organisation and its owner, or finds them again, and prints one single-use
 * invite with which the owner's first workstation pairs itself. Root only, on the API host:
 *
 *   bun run auth:owner ORG [--name NAME] [--email EMAIL] [--expires-in MINUTES] [--reactivate [--keep-devices]] [--revoke-devices]
 *
 * A rerun prints a fresh invite and revokes the owner's earlier pending ones; with
 * --revoke-devices it also revokes the owner's device credentials, for a lost workstation.
 * --reactivate restores a deactivated owner; if they have devices that were never revoked, it
 * also needs --revoke-devices or --keep-devices.
 */
import { parseArgs } from "node:util";
import { Effect, Schema } from "effect";
import { reservedUsernames } from "../src/config";
import { databaseLayer, databaseUrl } from "../src/db";
import {
  bootstrapOwner,
  BootstrapRefused,
  defaultInviteLifetime,
  deviceChoice,
  maxInviteLifetime,
  minInviteLifetime,
} from "../src/pairing";
import { Email, Name } from "../src/validation";

const usage =
  "Usage: bun run auth:owner ORG [--name NAME] [--email EMAIL] [--expires-in MINUTES] [--reactivate [--keep-devices]] [--revoke-devices]";

function fail(message: string): never {
  console.error(message);
  process.exit(1);
}

// The invite pairs a device of an administrator: only the host's root may print one.
if (process.getuid?.() !== 0) fail("Run this command as root on the API host.");

const { values, positionals } = parseArgs({
  args: process.argv.slice(2),
  allowPositionals: true,
  options: {
    name: { type: "string" },
    email: { type: "string" },
    "expires-in": { type: "string" },
    reactivate: { type: "boolean" },
    "keep-devices": { type: "boolean", default: false },
    "revoke-devices": { type: "boolean", default: false },
  },
});

const [organisation, ...extra] = positionals;
if (!organisation || extra.length || !/^[a-z0-9][a-z0-9.-]{0,62}$/u.test(organisation)) {
  fail(`${usage}\nORG is 1 to 63 lower-case letters, digits, dots and hyphens.`);
}
const minutes = values["expires-in"] ?? String(defaultInviteLifetime / 60);
const lifetime = Number(minutes) * 60;
if (!/^\d+$/u.test(minutes) || lifetime < minInviteLifetime || lifetime > maxInviteLifetime) {
  fail(`--expires-in is ${minInviteLifetime / 60} to ${maxInviteLifetime / 60} minutes.`);
}
let devices: ReturnType<typeof deviceChoice>;
try {
  devices = deviceChoice({
    reactivate: values.reactivate,
    revokeDevices: values["revoke-devices"],
    keepDevices: values["keep-devices"],
  });
} catch (error) {
  fail(error instanceof BootstrapRefused ? error.message : String(error));
}
const name = values.name === undefined ? undefined : Schema.decodeUnknownSync(Name)(values.name);
const email =
  values.email === undefined ? undefined : Schema.decodeUnknownSync(Email)(values.email);

try {
  const result = await Effect.runPromise(
    bootstrapOwner({
      externalOrganisation: organisation,
      displayName: name,
      email,
      lifetime,
      reactivate: values.reactivate,
      devices,
      reserved: reservedUsernames(process.env),
    }).pipe(Effect.provide(databaseLayer(databaseUrl()))),
  );
  const { owner } = result;
  console.log(
    `${result.organisationCreated ? "Created" : "Found"} organisation pairing:${organisation}.`,
  );
  console.log(
    `${result.ownerCreated ? "Created" : result.ownerReactivated ? "Reactivated" : "Found"} owner ${owner.displayName}${owner.username ? ` (${owner.username})` : ""}, member ${owner.id}.`,
  );
  if (result.revokedInvites) console.log(`Revoked ${result.revokedInvites} earlier invite(s).`);
  if (result.revokedDevices) console.log(`Revoked ${result.revokedDevices} device credential(s).`);
  if (result.keptDevices)
    console.log(`Kept ${result.keptDevices} device credential(s): they can sign in again.`);
  else if (result.ownerReactivated && !result.revokedDevices)
    console.log("No device credential signs in again.");
  console.log(
    `Single-use pairing invite, valid until ${result.invite.expiresAt.toISOString()}. Give it only to the owner:`,
  );
  console.log(result.invite.token);
} catch (error) {
  fail(
    error instanceof BootstrapRefused
      ? error.message
      : "The owner could not be bootstrapped. Check the database configuration and migrations.",
  );
}
