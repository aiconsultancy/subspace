import { databaseUrl } from "./db";
import { githubConfig } from "./github-app";
import { isUuid } from "./validation";

export type Provider = "workos" | "entra" | "dev";

export function authProviders(env: NodeJS.ProcessEnv): ReadonlySet<string> {
  const providers = new Set((env.AUTH_PROVIDERS ?? "workos").split(",").map((name) => name.trim()));
  if ([...providers].some((name) => !["workos", "entra", "dev"].includes(name))) {
    throw new Error("AUTH_PROVIDERS must contain workos, entra or dev");
  }
  if (providers.has("dev") && env.NODE_ENV === "production") {
    throw new Error("Dev auth is only available outside production");
  }
  if (providers.has("dev") && env.ENABLE_DEV_AUTH !== "true") {
    throw new Error("Dev auth requires ENABLE_DEV_AUTH=true");
  }
  return providers;
}

export function required(env: NodeJS.ProcessEnv, name: string) {
  const value = env[name]?.trim();
  if (!value) throw new Error(`${name} is required`);
  return value;
}

/** A WorkOS client ID as the dashboard shows it. The desktop's
 * `SUBSPACE_WORKOS_CLIENT_ID` and `workos_client_id` use the same rule. */
export const WORKOS_CLIENT_ID_PATTERN = /^client_[A-Za-z0-9]{1,121}$/;

function workosClientId(env: NodeJS.ProcessEnv) {
  const value = required(env, "WORKOS_CLIENT_ID");
  if (!WORKOS_CLIENT_ID_PATTERN.test(value)) {
    throw new Error("WORKOS_CLIENT_ID must be client_ followed by letters and digits");
  }
  return value;
}

function requiredUuid(env: NodeJS.ProcessEnv, name: string) {
  const value = required(env, name).toLowerCase();
  if (!isUuid(value)) throw new Error(`${name} must be a UUID`);
  return value;
}

function integerSetting(
  env: NodeJS.ProcessEnv,
  name: string,
  fallback: number,
  min: number,
  max: number,
) {
  const value = Number(env[name] ?? fallback);
  if (!Number.isInteger(value) || value < min || value > max) {
    throw new Error(`${name} must be an integer between ${min} and ${max}`);
  }
  return value;
}

export function config(env: NodeJS.ProcessEnv = process.env) {
  const providers = authProviders(env);
  const port = Number(env.PORT ?? "3000");
  if (!Number.isInteger(port) || port < 0 || port > 65535) {
    throw new Error("PORT must be a valid TCP port");
  }
  const hostname = env.HOST ?? "127.0.0.1";
  if (
    providers.has("dev") &&
    !["127.0.0.1", "::1", "localhost"].includes(hostname) &&
    env.DEV_CONTAINER_LOCAL_ONLY !== "true"
  ) {
    throw new Error(
      "Dev sign-in needs a loopback listener or local-only container port publishing",
    );
  }
  const ircPort = Number(env.DEV_IRC_PORT ?? "6667");
  if (!Number.isInteger(ircPort) || ircPort < 1 || ircPort > 65535) {
    throw new Error("DEV_IRC_PORT must be a valid TCP port");
  }

  return {
    databaseUrl: databaseUrl(env),
    databasePoolSize: integerSetting(env, "DATABASE_POOL_SIZE", 10, 2, 1000),
    updates: {
      perUser: integerSetting(env, "SSE_MAX_STREAMS_PER_USER", 16, 1, 1000),
      total: integerSetting(env, "SSE_MAX_STREAMS", 256, 1, 10_000),
    },
    github: githubConfig(env),
    hostname,
    port,
    dev: providers.has("dev")
      ? {
          keyPath: env.DEV_KEY_PATH ?? ".data/dev-signing-key",
          channels: env.DEV_CHANNELS ?? "#subspace",
          ircHost: env.DEV_IRC_HOST ?? "127.0.0.1",
          ircPort,
        }
      : null,
    workos: providers.has("workos")
      ? {
          clientId: workosClientId(env),
          issuer: env.WORKOS_ISSUER?.trim() || "https://api.workos.com/",
          audience: env.WORKOS_AUDIENCE?.trim() || undefined,
          jwksUrl: env.WORKOS_JWKS_URL?.trim() || undefined,
        }
      : null,
    entra: providers.has("entra")
      ? {
          tenantId: requiredUuid(env, "ENTRA_TENANT_ID"),
          clientId: requiredUuid(env, "ENTRA_CLIENT_ID"),
          audience: requiredUuid(env, "ENTRA_AUDIENCE"),
          scope: env.ENTRA_SCOPE ?? "API.Access",
        }
      : null,
  };
}

export type Config = ReturnType<typeof config>;
