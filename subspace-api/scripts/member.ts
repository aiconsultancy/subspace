import { parseArgs } from "node:util";
import { Effect, Schema } from "effect";
import { type PrincipalIdentity } from "../src/auth";
import { databaseLayer, databaseUrl } from "../src/db";
import { provisionMember } from "../src/identity";
import { Email, Name, Uuid } from "../src/validation";

const { values, positionals } = parseArgs({
  args: process.argv.slice(2),
  allowPositionals: true,
  options: {
    name: { type: "string" },
    email: { type: "string" },
    role: { type: "string", default: "member" },
    deactivate: { type: "boolean" },
  },
});

const [provider, externalOrganisation, subject, ...extra] = positionals;
const role = values.role;
if (
  !["workos", "entra"].includes(provider ?? "") ||
  !externalOrganisation ||
  !subject ||
  extra.length ||
  (role !== "admin" && role !== "member") ||
  externalOrganisation.length > 256 ||
  subject.length > 256
) {
  throw new Error(
    "Usage: bun run auth:member workos|entra ORG SUBJECT [--name NAME] [--email EMAIL] [--role member|admin] [--deactivate]",
  );
}

const id = Schema.decodeUnknownSync(Uuid);
const identity: PrincipalIdentity =
  provider === "entra"
    ? {
        provider,
        externalOrganisation: id(externalOrganisation).toLowerCase(),
        subject: id(subject).toLowerCase(),
      }
    : { provider: "workos", externalOrganisation, subject };
const readName = Schema.decodeUnknownSync(Name);
const readEmail = Schema.decodeUnknownSync(Email);
let name = values.name;
let email = values.email;

if (
  !values.deactivate &&
  (!name || !email) &&
  identity.provider === "workos" &&
  process.env.WORKOS_API_KEY
) {
  const response = await fetch(
    `https://api.workos.com/user_management/users/${encodeURIComponent(identity.subject)}`,
    {
      headers: { Authorization: `Bearer ${process.env.WORKOS_API_KEY}` },
      signal: AbortSignal.timeout(5000),
    },
  );
  if (!response.ok) throw new Error("WorkOS profile lookup failed");
  const profile = (await response.json()) as {
    first_name?: string;
    last_name?: string;
    email?: string;
  };
  name ??= [profile.first_name, profile.last_name].filter(Boolean).join(" ") || undefined;
  email ??= profile.email;
}
if (!values.deactivate && !name)
  throw new Error("Provide --name or configure WORKOS_API_KEY for lookup.");
const profile = {
  displayName: readName(name ?? "Inactive member"),
  email: email ? readEmail(email) : null,
};

try {
  await Effect.runPromise(
    provisionMember(identity, profile, role, !values.deactivate).pipe(
      Effect.provide(databaseLayer(databaseUrl())),
    ),
  );
  console.log(values.deactivate ? "Membership deactivated." : "Membership saved.");
} catch {
  console.error(
    "The membership could not be saved. Check the database configuration and migrations.",
  );
  process.exitCode = 1;
}
