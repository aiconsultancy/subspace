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

/**
 * The providers whose first valid token registers an unknown person as a member. Entra is on by
 * default when it is configured, because its tenant, audience and client are already pinned.
 * WorkOS needs an organisation allow-list first, and development sign-in registers its own
 * members, so neither is accepted. An empty value or `none` turns registration off.
 */
export function autoRegister(
  env: NodeJS.ProcessEnv,
  providers: ReadonlySet<string>,
): ReadonlySet<Provider> {
  const raw = env.AUTO_REGISTER;
  if (raw === undefined) return new Set<Provider>(providers.has("entra") ? ["entra"] : []);
  const names = raw
    .split(",")
    .map((name) => name.trim())
    .filter(Boolean);
  if (names.length === 0 || (names.length === 1 && names[0] === "none")) return new Set();
  for (const name of names) {
    if (name === "workos") {
      throw new Error(
        "AUTO_REGISTER does not accept workos yet: WorkOS registration needs an organisation allow-list",
      );
    }
    if (name !== "entra") throw new Error("AUTO_REGISTER must be entra, none or empty");
    if (!providers.has(name)) {
      throw new Error("AUTO_REGISTER must name a provider in AUTH_PROVIDERS");
    }
  }
  return new Set(names as Provider[]);
}

/** The Entra app role that makes a person an administrator, checked on every request. */
function entraAdminRole(env: NodeJS.ProcessEnv, providers: ReadonlySet<string>) {
  const value = env.ENTRA_ADMIN_ROLE?.trim();
  if (!value) return null;
  if (!providers.has("entra")) throw new Error("ENTRA_ADMIN_ROLE needs entra in AUTH_PROVIDERS");
  if (value.length > 256 || /\s/u.test(value)) {
    throw new Error("ENTRA_ADMIN_ROLE must be one app role value without whitespace");
  }
  return value;
}

/**
 * Operator names no person may be given as a username, beside the built-in IRC service names:
 * a comma-separated list compared without regard to case.
 */
export function reservedUsernames(env: NodeJS.ProcessEnv): ReadonlySet<string> {
  const names = (env.RESERVED_USERNAMES ?? "")
    .split(",")
    .map((name) => name.trim().toLowerCase())
    .filter(Boolean);
  for (const name of names) {
    if (!/^[a-z][a-z0-9_-]{0,63}$/u.test(name)) {
      throw new Error(
        "RESERVED_USERNAMES must be comma-separated names of a letter then letters, digits, - or _",
      );
    }
  }
  return new Set(names);
}

const loopback = ["127.0.0.1", "::1", "localhost"];

/**
 * The optional agent-only listener. A request on it names its registered agent in a header and
 * needs no token, so it trusts its network as development sign-in does: loopback, or local-only
 * container port publishing, and never in production.
 */
function agentListener(
  env: NodeJS.ProcessEnv,
  providers: ReadonlySet<string>,
  mainHostname: string,
  mainPort: number,
) {
  const raw = env.AGENT_PORT?.trim();
  if (!raw) return null;
  const port = Number(raw);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error("AGENT_PORT must be a valid TCP port");
  }
  if (port === mainPort) throw new Error("AGENT_PORT must differ from PORT");
  if (env.NODE_ENV === "production") {
    throw new Error("The agent listener is only available outside production");
  }
  const hostname = env.AGENT_HOST?.trim() || mainHostname;
  if (!loopback.includes(hostname) && env.DEV_CONTAINER_LOCAL_ONLY !== "true") {
    throw new Error(
      "The agent listener needs a loopback address or local-only container port publishing",
    );
  }
  const organisation = env.AGENT_ORGANISATION?.trim() || (providers.has("dev") ? "dev:local" : "");
  const separator = organisation.indexOf(":");
  const provider = organisation.slice(0, separator);
  const externalId = organisation.slice(separator + 1);
  if (separator < 1 || !externalId || !["workos", "entra", "dev"].includes(provider)) {
    throw new Error("AGENT_ORGANISATION must be <provider>:<organisation>, such as dev:local");
  }
  if (!providers.has(provider)) {
    throw new Error("AGENT_ORGANISATION must name a provider in AUTH_PROVIDERS");
  }
  return { hostname, port, provider: provider as Provider, externalOrganisation: externalId };
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
    !loopback.includes(hostname) &&
    env.DEV_CONTAINER_LOCAL_ONLY !== "true"
  ) {
    throw new Error(
      "Dev sign-in needs a loopback listener or local-only container port publishing",
    );
  }
  const agent = agentListener(env, providers, hostname, port);
  const adminRole = entraAdminRole(env, providers);
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
    agent,
    autoRegister: autoRegister(env, providers),
    reservedUsernames: reservedUsernames(env),
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
          adminRole,
        }
      : null,
  };
}

export type Config = ReturnType<typeof config>;
