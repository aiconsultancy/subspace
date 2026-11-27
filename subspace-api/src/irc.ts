import { createHash, createHmac, timingSafeEqual } from "node:crypto";
import { and, eq, isNull } from "drizzle-orm";
import { ByteSize, Effect, Layer, Redacted, Schema } from "effect";
import { HttpIncomingMessage, HttpRouter, HttpServerResponse } from "effect/http";
import { HttpApiEndpoint, HttpApiGroup, OpenApi } from "effect/http-api";
import { fingerprintPattern, touchCredential } from "./agent-keys";
import { canonicalHost, type Config } from "./config";
import { Database } from "./db";
import { CurrentMember } from "./identity";
import { BearerAuth, ValidationErrors, versionedApi } from "./middleware";
import { endpointErrors, problem } from "./problems";
import { agentCredentials, organisations, users } from "./schema";
import { canonicalName } from "./usernames";
import { Host, Port } from "./validation";

/**
 * IRC sign-in through the API. A signed-in user asks for a short-lived credential and gives it to
 * the IRC server with SASL PLAIN; the server's auth-script asks this API whether it is good.
 *
 * A credential is `sirc1.` and the unpadded base64url encoding of 56 bytes: the user's 16-byte
 * UUID, the expiry as 8 bytes of seconds since the epoch (big-endian), and an HMAC-SHA256 keyed by
 * IRC_CREDENTIAL_SECRET over `sirc1.`, those 24 bytes and the canonical host of the IRC server it
 * was issued for. The host is not in the credential: the hook checks the signature against each
 * host in IRC_AUTH_SERVERS, so a credential works only on a server this deployment lists. It is
 * stateless, valid until the expiry and reusable until then. Clients treat it as opaque.
 */
export const ircCredentialPrefix = "sirc1.";
export const ircCredentialLifetimeSeconds = 600;
const bodyLength = 24;
const macLength = 32;
const credentialShape = /^sirc1\.[A-Za-z0-9_-]{75}$/u;

type Secret = Redacted.Redacted<string>;

function signature(secret: Secret, body: Uint8Array, host: string) {
  return createHmac("sha256", Redacted.value(secret))
    .update(ircCredentialPrefix)
    .update(body)
    .update(canonicalHost(host), "utf8")
    .digest();
}

/** A credential for `userId` on the IRC server at `host`, valid for ten minutes from `now`. */
export function mintIrcCredential(secret: Secret, userId: string, host: string, now = Date.now()) {
  const id = Buffer.from(userId.replaceAll("-", ""), "hex");
  if (id.length !== 16) throw new Error("An IRC credential needs a user UUID.");
  const expiresAt = Math.floor(now / 1000) + ircCredentialLifetimeSeconds;
  const body = Buffer.alloc(bodyLength);
  id.copy(body, 0);
  body.writeBigUInt64BE(BigInt(expiresAt), 16);
  return {
    password:
      ircCredentialPrefix +
      Buffer.concat([body, signature(secret, body, host)]).toString("base64url"),
    expiresAt: new Date(expiresAt * 1000),
  };
}

export type CredentialCheck =
  | { readonly ok: true; readonly userId: string }
  | { readonly ok: false; readonly reason: "malformed" | "signature" | "expired" };

/**
 * The user a credential names, if it is well formed, carries this deployment's signature for one
 * of `hosts`, compared in constant time, and has not expired at `now`. An expiry further ahead
 * than a fresh credential's is refused as well.
 */
export function readIrcCredential(
  secret: Secret,
  passphrase: string,
  hosts: readonly string[],
  now = Date.now(),
): CredentialCheck {
  if (!credentialShape.test(passphrase)) return { ok: false, reason: "malformed" };
  const encoded = passphrase.slice(ircCredentialPrefix.length);
  const raw = Buffer.from(encoded, "base64url");
  if (raw.length !== bodyLength + macLength || raw.toString("base64url") !== encoded)
    return { ok: false, reason: "malformed" };
  const body = raw.subarray(0, bodyLength);
  const given = raw.subarray(bodyLength);
  let signed = false;
  for (const host of new Set(hosts.map(canonicalHost)))
    signed = timingSafeEqual(signature(secret, body, host), given) || signed;
  if (!signed) return { ok: false, reason: "signature" };
  const expiresAt = body.readBigUInt64BE(16);
  const seconds = BigInt(Math.floor(now / 1000));
  if (expiresAt <= seconds || expiresAt > seconds + BigInt(ircCredentialLifetimeSeconds + 60))
    return { ok: false, reason: "expired" };
  const hex = body.subarray(0, 16).toString("hex");
  return {
    ok: true,
    userId: `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`,
  };
}

/** Whether two secrets are equal, compared in constant time whatever their lengths. */
export function sameSecret(given: string, expected: Secret) {
  const digest = (value: string) => createHash("sha256").update(value).digest();
  return timingSafeEqual(digest(given), digest(Redacted.value(expected)));
}

const IrcCredential = Schema.Struct({
  account: Schema.String,
  password: Schema.String,
  expiresAt: Schema.String,
}).annotate({ identifier: "IrcCredential" });

/** The IRC server a credential is for, as the client will connect to it. */
const IrcServer = Schema.Struct({ host: Host, port: Port }).annotate({ identifier: "IrcServer" });

/**
 * `POST /api/v1/me/irc-credential`, on the main listener only: the agent listener does not serve
 * this group, because anyone who reaches that listener can act as any agent.
 */
export const ircCredentialGroup = HttpApiGroup.make("ircCredentials")
  .add(
    HttpApiEndpoint.post("ircCredential", "/me/irc-credential", {
      payload: IrcServer,
      success: IrcCredential,
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      "Issues a short-lived password for the caller's IRC account on the IRC server at host and port, to give that server with SASL PLAIN as the account (optionally followed by @ and a device name). The password is opaque, at most 200 characters, valid for ten minutes, reusable until it expires and accepted only by that deployment's IRC servers. 404 when this server does not issue IRC credentials or does not sign people in to that IRC server; 409 for an agent without an IRC nick.",
    ),
  )
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

/** An ISO 8601 time in UTC to the second, such as 2026-11-23T21:10:00Z. */
const utcSeconds = (date: Date) => date.toISOString().replace(/\.\d{3}Z$/u, "Z");

/** Whether `servers` list the host, ignoring ASCII case, and the port where an entry names one. */
export function listedServer(
  servers: readonly { host: string; port: number | null }[],
  host: string,
  port: number,
) {
  const canonical = canonicalHost(host);
  return servers.some((entry) => entry.host === canonical && (entry.port ?? port) === port);
}

export const ircCredentialHandlers =
  (secret: Secret | null, servers: readonly { host: string; port: number | null }[]) =>
  (
    handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<
      typeof ircCredentialGroup
    >,
  ) =>
    handlers.handle("ircCredential", ({ payload }) =>
      Effect.gen(function* () {
        if (!secret)
          return yield* Effect.fail(problem(404, "This server does not issue IRC credentials."));
        if (!listedServer(servers, payload.host, payload.port))
          return yield* Effect.fail(problem(404, "No account sign-in for this server"));
        const member = yield* CurrentMember;
        // A member signed in with a key credential reaches IRC only if that credential may.
        if (member.keyCredential && !member.keyCredential.scopes.includes("irc"))
          return yield* Effect.fail(
            problem(403, "This key credential does not have the irc scope."),
          );
        if (!member.username)
          return yield* Effect.fail(
            problem(
              409,
              member.agentRank
                ? `Agent '${member.displayName}' has no IRC nick. Ask an administrator to set one.`
                : "This account has no username yet. Sign in again shortly.",
            ),
          );
        const { password, expiresAt } = mintIrcCredential(secret, member.id, payload.host);
        return { account: member.username, password, expiresAt: utcSeconds(expiresAt) };
      }),
    );

export const ircAuthenticatePath = "/api/v1/irc/authenticate";

type Hook = NonNullable<Config["irc"]["hook"]>;

/** Why the hook refused, for a debug log line. Never the passphrase or the token. */
type Refusal =
  | (CredentialCheck & { ok: false })
  | {
      readonly ok: false;
      readonly reason:
        | "body"
        | "no credential"
        | "certificate"
        | "unknown certificate"
        | "scope"
        | "not an agent"
        | "account"
        | "unknown user"
        | "inactive"
        | "organisation"
        | "no username"
        | "error";
    };

const refused = (reason: Refusal["reason"]) => ({ ok: false as const, reason });

/** Ergo's auth-script input, of which only these fields are read; the rest are ignored. */
function readInput(text: string) {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    return undefined;
  }
  if (typeof value !== "object" || value === null || Array.isArray(value)) return undefined;
  const field = (key: string) => {
    const item = (value as Record<string, unknown>)[key];
    return typeof item === "string" ? item : undefined;
  };
  return {
    accountName: field("accountName"),
    passphrase: field("passphrase"),
    certfp: field("certfp"),
  };
}

/**
 * The account a client certificate signs in to: its fingerprint, compared as hex without regard to
 * case, must name an active key credential with scope `irc` of an active agent with an IRC nick,
 * in the hook's organisation. The answer is the agent's nick as registered.
 */
const certificateSignIn = (hook: Hook, certfp: string) =>
  Effect.gen(function* () {
    const fingerprint = certfp.toLowerCase();
    if (!fingerprintPattern.test(fingerprint)) return refused("certificate");
    const db = yield* Database;
    const rows = yield* db
      .select({
        credentialId: agentCredentials.id,
        scopes: agentCredentials.scopes,
        userId: users.id,
        agentRank: users.agentRank,
        active: users.active,
        username: users.username,
      })
      .from(agentCredentials)
      .innerJoin(
        users,
        and(
          eq(users.organisationId, agentCredentials.organisationId),
          eq(users.id, agentCredentials.userId),
        ),
      )
      .innerJoin(organisations, eq(organisations.id, agentCredentials.organisationId))
      .where(
        and(
          eq(agentCredentials.fingerprint, fingerprint),
          isNull(agentCredentials.revokedAt),
          eq(organisations.provider, hook.provider),
          eq(organisations.externalId, hook.externalOrganisation),
        ),
      )
      .limit(2);
    if (rows.length !== 1) return refused("unknown certificate");
    const row = rows[0]!;
    const rejection = !row.scopes.includes("irc")
      ? "scope"
      : !row.agentRank
        ? "not an agent"
        : !row.active
          ? "inactive"
          : !row.username
            ? "no username"
            : null;
    if (rejection) return { ...refused(rejection), userId: row.userId };
    yield* touchCredential(row.credentialId);
    return { ok: true as const, accountName: row.username!, userId: row.userId };
  });

/** The account a credential signs in to, or why it may not. */
const decide = (hook: Hook, secret: Secret, hosts: readonly string[], text: string) =>
  Effect.gen(function* () {
    const input = readInput(text);
    if (!input) return refused("body");
    if (input.passphrase === undefined || input.passphrase === "")
      return input.certfp ? yield* certificateSignIn(hook, input.certfp) : refused("no credential");
    if (!input.passphrase.startsWith(ircCredentialPrefix)) return refused("no credential");
    const credential = readIrcCredential(secret, input.passphrase, hosts);
    if (!credential.ok) return credential;
    const db = yield* Database;
    const [user] = yield* db
      .select({
        active: users.active,
        username: users.username,
        provider: organisations.provider,
        externalId: organisations.externalId,
      })
      .from(users)
      .innerJoin(organisations, eq(organisations.id, users.organisationId))
      .where(eq(users.id, credential.userId));
    if (!user) return { ...refused("unknown user"), userId: credential.userId };
    const rejection = !user.active
      ? "inactive"
      : user.provider !== hook.provider || user.externalId !== hook.externalOrganisation
        ? "organisation"
        : !user.username
          ? "no username"
          : canonicalName((input.accountName ?? "").split("@", 1)[0]!) !==
              canonicalName(user.username)
            ? "account"
            : null;
    if (rejection) return { ...refused(rejection), userId: credential.userId };
    return { ok: true as const, accountName: user.username!, userId: credential.userId };
  });

const noStore = { "Cache-Control": "no-store" };

/**
 * `POST /api/v1/irc/authenticate`, for Ergo's auth-script only: on the main listener, outside the
 * OpenAPI document, the MCP tools and the bearer middleware. Only the hook token authorises it.
 * Once the token is accepted the answer is always 200, `{"success":true,"accountName":...}` or
 * `{"success":false}`, so Ergo falls back to its own accounts for anything that is not a good
 * credential of this organisation: a `sirc1.` passphrase, or, with no passphrase, the certfp of an
 * agent's key credential with scope `irc`.
 */
export function ircAuthenticateRoute(hook: Hook, secret: Secret, hosts: readonly string[]) {
  return HttpRouter.add("POST", ircAuthenticatePath, (request) =>
    Effect.gen(function* () {
      const bearer = /^Bearer ([^\s]+)$/iu.exec(request.headers.authorization ?? "")?.[1];
      if (!bearer || !sameSecret(bearer, hook.token))
        return HttpServerResponse.jsonUnsafe(
          problem(401, "A valid IRC auth hook token is required."),
          {
            status: 401,
            contentType: "application/problem+json",
            headers: { ...noStore, "WWW-Authenticate": "Bearer" },
          },
        );
      const decision = yield* request.text.pipe(
        Effect.provideService(HttpIncomingMessage.MaxBodySize, ByteSize.kibibytes(256)),
        Effect.flatMap((text) => decide(hook, secret, hosts, text)),
        // A failed read or query, or a defect, is a refusal: the answer stays 200.
        Effect.catchCause(() => Effect.succeed(refused("error"))),
      );
      if (decision.ok) {
        yield* Effect.logDebug("IRC sign-in accepted", { userId: decision.userId });
        return HttpServerResponse.jsonUnsafe(
          { success: true, accountName: decision.accountName },
          { headers: noStore },
        );
      }
      yield* Effect.logDebug("IRC sign-in refused", {
        reason: decision.reason,
        ...("userId" in decision ? { userId: decision.userId } : {}),
      });
      return HttpServerResponse.jsonUnsafe({ success: false }, { headers: noStore });
    }),
  );
}

/** The hook's route when the hook token, organisation, credential secret and servers are set. */
export const ircAuthenticateLayer = (irc: Config["irc"]) =>
  irc.hook && irc.credentialSecret && irc.servers.length > 0
    ? ircAuthenticateRoute(
        irc.hook,
        irc.credentialSecret,
        irc.servers.map((server) => server.host),
      )
    : Layer.empty;
