// Root Compose only. Use the normal signed webhook endpoint for the bundled
// GitHub fixtures, with a private local key and no GitHub App or network calls.
import { Effect } from "effect";
import { readDevKey } from "../src/auth";
import { config } from "../src/config";
import { databaseLayer } from "../src/db";
import { localMember } from "../src/identity";
import { defaults, webhookKeyPath } from "./demo/settings";

const options = config();
if (options.dev && !options.workos && !options.entra && !options.github.organisationId) {
  const member = await Effect.runPromise(
    localMember(defaults.email, defaults.name, defaults.organisation, options.dev).pipe(
      Effect.provide(databaseLayer(options.databaseUrl)),
    ),
  );
  process.env.GITHUB_ORGANISATION_ID = member.organisationId;
  process.env.GITHUB_WEBHOOK_SECRET = (await readDevKey(webhookKeyPath)).toString("hex");
}

await import("../src/main");
