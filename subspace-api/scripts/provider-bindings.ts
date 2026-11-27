/**
 * Lists the WorkOS organisations and Entra tenants whose accounts are linked into each Subspace
 * organisation, or releases one from a Subspace organisation by unlinking its accounts there.
 * Root only, on the API host:
 *
 *   bun run auth:bindings
 *   bun run auth:bindings --release workos|entra|dev|pairing ORG workos|entra PROVIDER_ORG
 *
 * A provider organisation whose first account was linked into a Subspace organisation other than
 * its own is bound to it: its unknown people are refused at first sign-in rather than registered
 * in an organisation of their own. Releasing it unlinks every one of its accounts in that
 * organisation, audited; those people then sign in there only with their devices.
 */
import { parseArgs } from "node:util";
import { Effect, Schema } from "effect";
import { type Database, databaseLayer, databaseUrl } from "../src/db";
import {
  providerOrganisationBindings,
  releaseProviderOrganisation,
  ReleaseRefused,
} from "../src/provider-organisations";
import { Uuid } from "../src/validation";

const usage =
  "Usage: bun run auth:bindings\n" +
  "       bun run auth:bindings --release workos|entra|dev|pairing ORG workos|entra PROVIDER_ORG";

function fail(message: string): never {
  console.error(message);
  process.exit(1);
}

// A release changes who can sign in to an organisation: only the host's root may run this.
if (process.getuid?.() !== 0) fail("Run this command as root on the API host.");

const { values, positionals } = parseArgs({
  args: process.argv.slice(2),
  allowPositionals: true,
  options: { release: { type: "boolean", default: false } },
});

const providers = ["workos", "entra", "dev", "pairing"] as const;
const isProvider = (value: string | undefined): value is (typeof providers)[number] =>
  providers.includes(value as never);
/** An Entra tenant ID in the lower case the API stores; any other ID as given. */
const externalId = (provider: string, value: string) =>
  provider === "entra" ? Schema.decodeUnknownSync(Uuid)(value).toLowerCase() : value;

const run = <A, E>(effect: Effect.Effect<A, E, Database>) =>
  Effect.runPromise(effect.pipe(Effect.provide(databaseLayer(databaseUrl()))));

if (!values.release) {
  if (positionals.length) fail(usage);
  try {
    const bindings = await run(providerOrganisationBindings);
    if (!bindings.length) console.log("No provider account is linked in any organisation.");
    for (const binding of bindings)
      console.log(
        `${binding.organisation.provider}:${binding.organisation.externalId} <- ${binding.provider}:${binding.externalOrganisation}, ${binding.accounts} linked account(s)${binding.own ? " (its own organisation: binds nothing)" : ""}`,
      );
  } catch {
    fail("The bindings could not be listed. Check the database configuration and migrations.");
  }
} else {
  const [organisationProvider, organisation, provider, providerOrganisation, ...extra] =
    positionals;
  if (
    !isProvider(organisationProvider) ||
    !organisation ||
    (provider !== "workos" && provider !== "entra") ||
    !providerOrganisation ||
    extra.length ||
    organisation.length > 256 ||
    providerOrganisation.length > 256
  )
    fail(usage);
  let released: string[];
  try {
    released = await run(
      releaseProviderOrganisation(
        {
          provider: organisationProvider,
          externalId: externalId(organisationProvider, organisation),
        },
        { provider, externalOrganisation: externalId(provider, providerOrganisation) },
      ),
    );
  } catch (error) {
    fail(
      error instanceof ReleaseRefused
        ? error.message
        : "The binding could not be released. Check the database configuration and migrations.",
    );
  }
  console.log(
    released.length
      ? `Unlinked ${released.length} account(s) of ${provider}:${providerOrganisation} from ${organisationProvider}:${organisation}; it is no longer bound there.`
      : `No account of ${provider}:${providerOrganisation} is linked in ${organisationProvider}:${organisation}.`,
  );
}
