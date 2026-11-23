import { readFileSync } from "node:fs";
import { Redacted } from "effect";
import { databaseUrl } from "./db";
import { canonicalName } from "./usernames";
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
 * The providers whose first valid token registers an unknown person as a member. It is off
 * unless listed, so an upgrade never widens who has access. Only `entra` is accepted: WorkOS
 * needs an organisation allow-list first, and development sign-in registers its own members.
 * An empty value or `none` is off.
 */
export function autoRegister(
  env: NodeJS.ProcessEnv,
  providers: ReadonlySet<string>,
): ReadonlySet<Provider> {
  const names = (env.AUTO_REGISTER ?? "")
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

/** Whether first sign-in registration may register an Entra guest. Off unless `true`. */
function entraRegisterGuests(env: NodeJS.ProcessEnv) {
  const value = env.ENTRA_REGISTER_GUESTS?.trim() || "false";
  if (value !== "true" && value !== "false") {
    throw new Error("ENTRA_REGISTER_GUESTS must be true or false");
  }
  return value === "true";
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
 * Names no person or agent may be given as a username, beside the built-in IRC service names:
 * operator names and the IRC server's existing accounts. RESERVED_USERNAMES is a comma-separated
 * list, and RESERVED_USERNAMES_FILE a file of names separated by commas, spaces or new lines, in
 * which a line starting with # is a comment. Both may be set. Names compare in ASCII lower case.
 */
export function reservedUsernames(env: NodeJS.ProcessEnv): ReadonlySet<string> {
  const file = env.RESERVED_USERNAMES_FILE?.trim();
  let listed = "";
  if (file) {
    try {
      listed = readFileSync(file, "utf8");
    } catch {
      throw new Error("RESERVED_USERNAMES_FILE must name a readable file");
    }
  }
  const names = [
    ...(env.RESERVED_USERNAMES ?? "").split(","),
    ...listed
      .split(/\r?\n/u)
      .filter((line) => !line.trim().startsWith("#"))
      .flatMap((line) => line.split(/[\s,]+/u)),
  ]
    .map((name) => canonicalName(name.trim()))
    .filter(Boolean);
  for (const name of names) {
    if (!/^[\x21-\x7e]{1,64}$/u.test(name)) {
      throw new Error(
        "RESERVED_USERNAMES and RESERVED_USERNAMES_FILE must hold names of up to 64 printable ASCII characters",
      );
    }
  }
  return new Set(names);
}

/**
 * A secret from the environment or from a file named in `<name>_FILE`, trimmed, of at least
 * `minimumBytes` bytes. Null when neither is set; an error for both, a missing or empty file, or
 * a short secret.
 */
function secretSetting(env: NodeJS.ProcessEnv, name: string, minimumBytes: number) {
  const value = env[name]?.trim();
  const file = env[`${name}_FILE`]?.trim();
  if (value && file) throw new Error(`Set ${name} or ${name}_FILE, not both`);
  let secret = value;
  if (file) {
    try {
      secret = readFileSync(file, "utf8").trim();
    } catch {
      throw new Error(`${name}_FILE must name a readable file`);
    }
    if (!secret) throw new Error(`${name}_FILE must not be empty`);
  }
  if (!secret) return null;
  if (Buffer.byteLength(secret, "utf8") < minimumBytes) {
    throw new Error(`${name} must be at least ${minimumBytes} bytes`);
  }
  return secret;
}

/** An IRC server's host in canonical form: ASCII lower case, without IPv6 brackets. */
export const canonicalHost = (host: string) =>
  canonicalName(host.startsWith("[") && host.endsWith("]") ? host.slice(1, -1) : host);

const hostShape = /^(?:[a-zA-Z0-9](?:[a-zA-Z0-9.-]*[a-zA-Z0-9])?|[0-9a-fA-F:.]*:[0-9a-fA-F:.]*)$/u;

/**
 * IRC_AUTH_SERVERS: the hosts, each optionally with a port, at which this deployment's IRC server
 * is reached, comma-separated, such as `irc.example.com:6697,[2001:db8::1]`. A credential is
 * issued only for one of them and is bound to its host.
 */
export function ircServers(value: string) {
  return value
    .split(",")
    .map((entry) => entry.trim())
    .filter(Boolean)
    .map((entry) => {
      const bracketed = /^\[([^\]]+)\](?::([0-9]+))?$/u.exec(entry);
      const plain = /^([^:]+)(?::([0-9]+))?$/u.exec(entry);
      const [host, port] = bracketed
        ? [bracketed[1]!, bracketed[2]]
        : plain
          ? [plain[1]!, plain[2]]
          : [entry, undefined];
      const number = port === undefined ? null : Number(port);
      if (
        !hostShape.test(host) ||
        host.length > 253 ||
        (number !== null && (!Number.isInteger(number) || number < 1 || number > 65535))
      ) {
        throw new Error(
          "IRC_AUTH_SERVERS must be comma-separated hosts, each optionally with :port, such as irc.example.com:6697",
        );
      }
      return { host: canonicalHost(host), port: number };
    });
}

/**
 * IRC sign-in through the API: the secret that signs short-lived IRC credentials, the IRC servers
 * they may be issued for, and the hook Ergo's auth-script calls, with its bearer token and the one
 * organisation its server serves. Setting only some of them is refused rather than left inert.
 */
function ircSignIn(env: NodeJS.ProcessEnv, providers: ReadonlySet<string>) {
  const credentialSecret = secretSetting(env, "IRC_CREDENTIAL_SECRET", 32);
  const hookToken = secretSetting(env, "IRC_AUTH_HOOK_TOKEN", 32);
  const organisation = env.IRC_AUTH_ORGANISATION?.trim();
  const servers = ircServers(env.IRC_AUTH_SERVERS ?? "");
  if (Boolean(hookToken) !== Boolean(organisation)) {
    throw new Error("Set IRC_AUTH_HOOK_TOKEN and IRC_AUTH_ORGANISATION together");
  }
  if (hookToken && !credentialSecret) {
    throw new Error("The IRC auth hook needs IRC_CREDENTIAL_SECRET");
  }
  if (credentialSecret && servers.length === 0) {
    throw new Error("IRC_CREDENTIAL_SECRET needs IRC_AUTH_SERVERS, the hosts of this IRC server");
  }
  if (!credentialSecret && servers.length > 0) {
    throw new Error("IRC_AUTH_SERVERS needs IRC_CREDENTIAL_SECRET");
  }
  if (hookToken && hookToken === credentialSecret) {
    throw new Error("IRC_AUTH_HOOK_TOKEN and IRC_CREDENTIAL_SECRET must be different secrets");
  }
  const secret = credentialSecret === null ? null : Redacted.make(credentialSecret);
  if (!hookToken || !organisation) return { credentialSecret: secret, servers, hook: null };
  if (!/^[\x21-\x7e]+$/u.test(hookToken)) {
    throw new Error("IRC_AUTH_HOOK_TOKEN must be printable ASCII without spaces");
  }
  const separator = organisation.indexOf(":");
  const provider = organisation.slice(0, separator);
  let externalOrganisation = organisation.slice(separator + 1);
  if (separator < 1 || !externalOrganisation || !["workos", "entra", "dev"].includes(provider)) {
    throw new Error("IRC_AUTH_ORGANISATION must be <provider>:<organisation>, such as dev:local");
  }
  if (!providers.has(provider)) {
    throw new Error("IRC_AUTH_ORGANISATION must name a provider in AUTH_PROVIDERS");
  }
  if (provider === "entra") {
    externalOrganisation = externalOrganisation.toLowerCase();
    if (!isUuid(externalOrganisation)) {
      throw new Error("IRC_AUTH_ORGANISATION must name an Entra tenant by its UUID");
    }
  }
  return {
    credentialSecret: secret,
    servers,
    hook: {
      token: Redacted.make(hookToken),
      provider: provider as Provider,
      externalOrganisation,
    },
  };
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
    irc: ircSignIn(env, providers),
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
          registerGuests: entraRegisterGuests(env),
        }
      : null,
  };
}

export type Config = ReturnType<typeof config>;
