import { createHash, X509Certificate, type KeyObject } from "node:crypto";
import { and, eq, isNull, sql } from "drizzle-orm";
import { Effect, Result, Schema } from "effect";
import { decodeProtectedHeader, importSPKI, jwtVerify, type JWTPayload } from "jose";
import { agentTokenIssuer, externalClockTolerance } from "./auth";
import { type Provider } from "./config";
import { Database } from "./db";
import { type AgentCredentialUse, type Member } from "./identity";
import { databaseFailure } from "./middleware";
import { problem } from "./problems";
import { agentCredentials, agentTokenIds, fleets, organisations, ships, users } from "./schema";

/**
 * Agent key credentials. An agent holds an X.509 certificate, normally self-signed, with an
 * Ed25519 or ECDSA P-256 key. Its fingerprint is the SHA-256 of the certificate's DER encoding in
 * lower-case hex, the value Ergo reports as certfp. Only the key and the fingerprint identify the
 * agent: no other field of the certificate is trusted, and its validity dates are not enforced,
 * because a self-signed certificate's dates are only what its holder asserted. Revocation is what
 * ends a key's use.
 *
 * An agent token is a JWT the agent signs with that key and sends as a bearer token to the main
 * listener: header `alg` (EdDSA or ES256), `typ` `subspace-agent+jwt` and `kid` the fingerprint;
 * claims `iss` `subspace-agent`, `sub` the fingerprint, `aud` the API's public origin
 * (AGENT_TOKEN_AUDIENCE), `iat`, `exp` at most 300 seconds after `iat`, and a unique `jti`, which
 * is accepted once.
 */
export const agentTokenType = "subspace-agent+jwt";
export const agentTokenMaxLifetime = 300;
export const maxCertificateLength = 16_384;
export const fingerprintPattern = /^[0-9a-f]{64}$/u;

export const CredentialScope = Schema.Literals(["api", "irc"]);
export type CredentialScope = typeof CredentialScope.Type;
export const CredentialScopes = Schema.Array(CredentialScope).check(
  Schema.isMinLength(1),
  Schema.isMaxLength(2),
  Schema.makeFilter((scopes) => new Set(scopes).size === scopes.length, {
    expected: "each scope at most once",
  }),
);
export const KeyAlgorithm = Schema.Literals(["EdDSA", "ES256"]);
export type KeyAlgorithm = typeof KeyAlgorithm.Type;
/** A certificate as PEM text: one `CERTIFICATE` block. */
export const CertificatePem = Schema.String.check(
  Schema.isMinLength(1),
  Schema.isMaxLength(maxCertificateLength),
);

export interface ParsedCertificate {
  /** The certificate re-encoded as PEM in 64-character lines. */
  readonly pem: string;
  readonly fingerprint: string;
  readonly algorithm: KeyAlgorithm;
  /** The public key as SPKI PEM. */
  readonly publicKey: string;
}

export type CertificateCheck =
  | { readonly ok: true; readonly certificate: ParsedCertificate }
  | { readonly ok: false; readonly reason: "malformed" | "key" };

const pemBlock =
  /^-----BEGIN CERTIFICATE-----\r?\n([A-Za-z0-9+/=\r\n]+?)\r?\n-----END CERTIFICATE-----$/u;

const keyAlgorithm = (key: KeyObject): KeyAlgorithm | undefined =>
  key.asymmetricKeyType === "ed25519"
    ? "EdDSA"
    : key.asymmetricKeyType === "ec" && key.asymmetricKeyDetails?.namedCurve === "prime256v1"
      ? "ES256"
      : undefined;

/**
 * Reads one PEM certificate strictly: a single `CERTIFICATE` block of canonical base64 whose bytes
 * are exactly one DER certificate, with nothing before, between or after but surrounding
 * whitespace. Its key must be Ed25519 or ECDSA P-256 and usable for verification; RSA, other
 * curves and every other key type are refused. Its names, extensions, issuer, signature and
 * validity dates are not read.
 */
export async function parseCertificate(input: string): Promise<CertificateCheck> {
  if (input.length > maxCertificateLength) return { ok: false, reason: "malformed" };
  const block = pemBlock.exec(input.trim());
  if (!block) return { ok: false, reason: "malformed" };
  const body = block[1]!.replace(/\r?\n/gu, "");
  if (!/^[A-Za-z0-9+/]+={0,2}$/u.test(body) || body.length % 4 !== 0)
    return { ok: false, reason: "malformed" };
  const der = Buffer.from(body, "base64");
  if (der.toString("base64") !== body) return { ok: false, reason: "malformed" };
  let certificate: X509Certificate;
  try {
    certificate = new X509Certificate(der);
  } catch {
    return { ok: false, reason: "malformed" };
  }
  // The parser reads one certificate and ignores anything after it: the whole input must be it.
  if (!Buffer.from(certificate.raw).equals(der)) return { ok: false, reason: "malformed" };
  let key: KeyObject;
  try {
    key = certificate.publicKey;
  } catch {
    return { ok: false, reason: "key" };
  }
  const algorithm = keyAlgorithm(key);
  if (!algorithm) return { ok: false, reason: "key" };
  const publicKey = key.export({ type: "spki", format: "pem" }).toString();
  try {
    await importSPKI(publicKey, algorithm);
  } catch {
    return { ok: false, reason: "key" };
  }
  const lines = der.toString("base64").replace(/(.{64})/gu, "$1\n");
  return {
    ok: true,
    certificate: {
      pem: `-----BEGIN CERTIFICATE-----\n${lines.replace(/\n$/u, "")}\n-----END CERTIFICATE-----\n`,
      fingerprint: createHash("sha256").update(der).digest("hex"),
      algorithm,
      publicKey,
    },
  };
}

export const certificateProblemDetail = {
  malformed: "Send one PEM certificate (-----BEGIN CERTIFICATE-----) in DER encoding.",
  key: "Use a certificate with an Ed25519 or ECDSA P-256 key.",
} as const;

/** The agent an agent token names, with the credential it was signed with and its expiry. */
export interface AgentPrincipal {
  readonly member: Member;
  readonly credential: AgentCredentialUse;
  readonly expiresAt: number;
}

const invalid = "The bearer access token is invalid or expired.";

/** A refusal of an agent token, with the reason for a debug log line. Never the token. */
const refuse = (reason: string, fingerprint?: string) =>
  Effect.logDebug("Agent token refused", { reason, ...(fingerprint ? { fingerprint } : {}) }).pipe(
    Effect.andThen(Effect.fail(problem(401, invalid))),
  );

/** A `jti`: 1 to 256 printable ASCII characters. */
const jtiShape = /^[\x21-\x7e]{1,256}$/u;

/** Whether the claims have the contract's shape beyond what the signature check covers. */
function claimsProblem(payload: JWTPayload, fingerprint: string, audience: string, now: number) {
  const { sub, aud, iat, exp, jti } = payload;
  if (typeof sub !== "string" || sub.toLowerCase() !== fingerprint) return "subject";
  if (typeof aud !== "string" || aud !== audience) return "audience";
  if (typeof iat !== "number" || typeof exp !== "number") return "times";
  if (!Number.isInteger(iat) || !Number.isInteger(exp)) return "times";
  if (exp <= iat || exp - iat > agentTokenMaxLifetime) return "lifetime";
  if (iat > now + externalClockTolerance) return "issued in the future";
  if (typeof jti !== "string" || !jtiShape.test(jti)) return "jti";
  return undefined;
}

/** Records a credential's use, at most once a minute, so that busy agents do not write each time. */
export const touchCredential = (id: string) =>
  Effect.flatMap(Database, (db) =>
    db
      .update(agentCredentials)
      .set({ lastUsedAt: sql`now()` })
      .where(
        and(
          eq(agentCredentials.id, id),
          sql`(${agentCredentials.lastUsedAt} is null or ${agentCredentials.lastUsedAt} < now() - interval '1 minute')`,
        ),
      ),
  );

/**
 * What an agent token must match: the API's public origin, null when agent tokens are off, and the
 * providers of the organisations this deployment serves.
 */
export interface AgentTokenPolicy {
  readonly audience: string | null;
  readonly providers: ReadonlySet<Provider>;
}

const checkAgentToken = (token: string, policy: AgentTokenPolicy) =>
  Effect.gen(function* () {
    const { audience } = policy;
    if (!audience) return yield* refuse("agent tokens are not configured");
    if (token.length > 8192) return yield* refuse("size");
    let header: ReturnType<typeof decodeProtectedHeader>;
    try {
      header = decodeProtectedHeader(token);
    } catch {
      return yield* refuse("header");
    }
    if (header.typ !== agentTokenType) return yield* refuse("typ");
    if (header.alg !== "EdDSA" && header.alg !== "ES256") return yield* refuse("alg");
    const fingerprint = typeof header.kid === "string" ? header.kid.toLowerCase() : "";
    if (!fingerprintPattern.test(fingerprint)) return yield* refuse("kid");

    const db = yield* Database;
    const rows = yield* db
      .select({
        credential: agentCredentials,
        member: users,
        provider: organisations.provider,
        fleet: fleets.slug,
        ship: ships.name,
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
      .leftJoin(fleets, eq(fleets.id, users.agentFleetId))
      .leftJoin(ships, eq(ships.id, users.agentShipId))
      .where(and(eq(agentCredentials.fingerprint, fingerprint), isNull(agentCredentials.revokedAt)))
      .limit(2);
    // The unique index allows one active credential per fingerprint; anything else is refused.
    if (rows.length !== 1) return yield* refuse("unknown credential", fingerprint);
    const { credential, member, provider, fleet, ship } = rows[0]!;
    if (!credential.scopes.includes("api")) return yield* refuse("scope", fingerprint);
    if (credential.algorithm !== header.alg) return yield* refuse("algorithm", fingerprint);

    const verified = yield* Effect.tryPromise(async () => {
      const key = await importSPKI(credential.publicKey, credential.algorithm);
      return jwtVerify(token, key, {
        issuer: agentTokenIssuer,
        audience,
        algorithms: [credential.algorithm],
        typ: agentTokenType,
        clockTolerance: externalClockTolerance,
        requiredClaims: ["iss", "sub", "aud", "iat", "exp", "jti"],
      });
    }).pipe(Effect.result);
    if (Result.isFailure(verified)) return yield* refuse("signature or claims", fingerprint);
    const { payload } = verified.success;
    const now = Math.floor(Date.now() / 1000);
    const claims = claimsProblem(payload, fingerprint, audience, now);
    if (claims) return yield* refuse(claims, fingerprint);

    if (!member.agentRank) return yield* refuse("not an agent", fingerprint);
    // An organisation whose identity provider this deployment does not accept is not served.
    if (!policy.providers.has(provider)) return yield* refuse("organisation", fingerprint);
    if (!member.active)
      return yield* Effect.fail(
        problem(403, `Agent '${member.displayName}' is deactivated. Ask a human to reactivate it.`),
      );

    // Recorded until it can no longer be accepted, so a replay to any instance is refused.
    const [fresh] = yield* db
      .insert(agentTokenIds)
      .values({
        credentialId: credential.id,
        jti: payload.jti!,
        expiresAt: new Date((payload.exp! + externalClockTolerance) * 1000),
      })
      .onConflictDoNothing()
      .returning({ jti: agentTokenIds.jti });
    if (!fresh) return yield* refuse("replayed jti", fingerprint);
    yield* touchCredential(credential.id);

    return {
      member: {
        ...member,
        agentFleet: fleet ?? member.agentFleet,
        agentShip: ship ?? member.agentShip,
      },
      credential: { id: credential.id, scopes: credential.scopes },
      expiresAt: payload.exp!,
    } satisfies AgentPrincipal;
  });

type Refusal = ReturnType<typeof problem<401>> | ReturnType<typeof problem<403>>;
type Failure = Refusal | ReturnType<typeof problem<500>>;

const isRefusal = (error: unknown): error is Refusal =>
  typeof error === "object" && error !== null && "type" in error && error.type === "about:blank";

/**
 * The agent an agent token names, if every rule of the contract holds, in this order: the header
 * (`typ`, an allowed `alg`, a fingerprint `kid`); one active credential with that fingerprint and
 * scope `api`, whose key's algorithm is the header's; the signature under that key; the issuer,
 * the audience, the subject, the times and the lifetime; an agent record of an organisation this
 * deployment serves; an active agent (403 otherwise); and a `jti` not seen before, recorded so
 * that no instance accepts it again. Everything else is 401, with the same detail whatever the
 * reason. A database failure is 500.
 */
export const verifyAgentToken = (token: string, policy: AgentTokenPolicy) =>
  checkAgentToken(token, policy).pipe(
    Effect.catch((error): Effect.Effect<never, Failure> =>
      isRefusal(error)
        ? Effect.fail(error)
        : Effect.logError("The agent token could not be checked", databaseFailure(error)).pipe(
            Effect.andThen(Effect.fail(problem(500, "The agent token could not be checked."))),
          ),
    ),
  );

/**
 * Removes the record of agent tokens that expired more than five minutes ago, by the database's
 * clock. The margin covers a difference between the database's and the instances' clocks, so a
 * token is always refused by its expiry before its record goes.
 */
export const purgeAgentTokenIds = Effect.flatMap(Database, (db) =>
  db.delete(agentTokenIds).where(sql`${agentTokenIds.expiresAt} < now() - interval '5 minutes'`),
);
