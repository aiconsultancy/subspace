import { randomBytes, randomUUID } from "node:crypto";
import { link, mkdir, readFile, unlink, writeFile } from "node:fs/promises";
import { dirname } from "node:path";
import {
  createRemoteJWKSet,
  customFetch,
  decodeJwt,
  jwtVerify,
  SignJWT,
  type JWTPayload,
} from "jose";
import { type Config, type Provider } from "./config";

/** Seconds of clock difference allowed on a token's times. */
export const externalClockTolerance = 5;
/** The issuer of agent tokens, which agents sign with their own keys (see `agent-keys.ts`). */
export const agentTokenIssuer = "subspace-agent";
/** The issuer of device tokens, which people's paired devices sign with their own keys. */
export const deviceTokenIssuer = "subspace-device";
/** Tokens signed with a member's registered key: never a provider's, whatever they claim. */
export const isKeyTokenIssuer = (issuer: string | undefined) =>
  issuer === agentTokenIssuer || issuer === deviceTokenIssuer;
const devIssuer = "subspace-dev";
const devAudience = "subspace-api";
export const devTokenLifetime = 28_800;

export type PrincipalIdentity = {
  provider: Provider;
  externalOrganisation: string;
  subject: string;
};

/**
 * Profile and role claims from a verified token. They fill in a new person's profile and decide
 * the administrator role; they never identify anyone, which only the provider, organisation and
 * subject do.
 */
export interface SignInClaims {
  readonly name?: string;
  readonly email?: string;
  readonly preferredUsername?: string;
  readonly givenName?: string;
  readonly familyName?: string;
  readonly roles: readonly string[];
  /** Whether the token names a guest of the tenant rather than a member of it. */
  readonly guest?: boolean;
}

export type VerifiedIdentity = PrincipalIdentity & {
  expiresAt: number;
  claims?: SignInClaims;
};
type Verifier = (token: string) => Promise<VerifiedIdentity>;

export async function readDevKey(path: string) {
  await mkdir(dirname(path), { recursive: true, mode: 0o700 });
  const readKey = async () => {
    const key = await readFile(path, "utf8");
    if (!/^[0-9a-f]{64}$/.test(key)) throw new Error("The dev signing key is invalid");
    return Buffer.from(key, "hex");
  };
  try {
    return await readKey();
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
  }
  const temporary = `${path}.${randomUUID()}`;
  try {
    await writeFile(temporary, randomBytes(32).toString("hex"), { mode: 0o600, flag: "wx" });
    await link(temporary, path);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "EEXIST") throw error;
  } finally {
    await unlink(temporary).catch((error: NodeJS.ErrnoException) => {
      if (error.code !== "ENOENT") throw error;
    });
  }
  return readKey();
}

export function mintDevToken(key: Uint8Array, subject: string, organisation: string) {
  return new SignJWT({ org_id: organisation })
    .setProtectedHeader({ alg: "HS256", typ: "JWT" })
    .setIssuer(devIssuer)
    .setAudience(devAudience)
    .setSubject(subject)
    .setIssuedAt()
    .setExpirationTime(`${devTokenLifetime}s`)
    .sign(key);
}

function claim(payload: JWTPayload, key: string) {
  const value = payload[key];
  if (typeof value !== "string" || !value || value.length > 256) {
    throw new Error("Missing or invalid token claim");
  }
  return value;
}

/** An optional profile claim: a non-empty string of at most 256 characters, or nothing. */
function profileClaim(payload: JWTPayload, key: string) {
  const value = payload[key];
  if (typeof value !== "string") return undefined;
  const text = value.trim();
  return text && text.length <= 256 ? text : undefined;
}

function signInClaims(payload: JWTPayload): SignInClaims {
  const roles = Array.isArray(payload.roles)
    ? payload.roles
        .filter((role): role is string => typeof role === "string" && role.length <= 256)
        .slice(0, 256)
    : [];
  return {
    name: profileClaim(payload, "name"),
    email: profileClaim(payload, "email"),
    preferredUsername: profileClaim(payload, "preferred_username"),
    givenName: profileClaim(payload, "given_name"),
    familyName: profileClaim(payload, "family_name"),
    roles,
  };
}

/** The fetch that reaches identity providers' discovery documents and keys. */
export type RemoteFetch = NonNullable<
  NonNullable<Parameters<typeof createRemoteJWKSet>[1]>[typeof customFetch]
>;

/**
 * Whether an Entra token names a guest. Microsoft documents `acct`, an optional claim, as 0 for a
 * member of the tenant and 1 for a guest, so when it is present it decides; any other value is
 * treated as a guest. Without it, `idp` is the fallback: Microsoft documents it as the same as the
 * issuer unless the account is not in the issuer's tenant, such as a guest, so a token is a
 * guest's when `idp` is present and is not this tenant's issuer, in its v2 or v1 form. A guest
 * whose account lives in this tenant cannot be told apart without `acct`.
 */
function entraGuest(payload: JWTPayload, tenantId: string) {
  if (payload.acct !== undefined) return !(payload.acct === 0 || payload.acct === "0");
  const issuers = [
    `https://login.microsoftonline.com/${tenantId}/v2.0`,
    `https://sts.windows.net/${tenantId}/`,
  ];
  const idp = payload.idp;
  return idp !== undefined && !(typeof idp === "string" && issuers.includes(idp.toLowerCase()));
}

/**
 * A token's issuer before any verification, or undefined. It only selects a verifier, which then
 * checks the signature and every claim; it never identifies anyone.
 */
export function unverifiedIssuer(token: string) {
  if (!token || token.length > 16_384) return undefined;
  try {
    const issuer = decodeJwt(token).iss;
    return typeof issuer === "string" ? issuer : undefined;
  } catch {
    return undefined;
  }
}

export function jwtVerifiers(
  options: Config,
  devKey: Uint8Array | null,
  fetchRemote: RemoteFetch = fetch,
) {
  const verifiers = new Map<string, Verifier>();

  if (options.workos) {
    const { issuer, clientId, jwksUrl, audience } = options.workos;
    const jwks = createRemoteJWKSet(
      new URL(jwksUrl ?? `https://api.workos.com/sso/jwks/${encodeURIComponent(clientId)}`),
      { [customFetch]: fetchRemote },
    );
    verifiers.set(issuer, async (token) => {
      const { payload } = await jwtVerify(token, jwks, {
        issuer,
        algorithms: ["RS256"],
        audience,
        clockTolerance: externalClockTolerance,
        requiredClaims: ["exp", "iat", "sub", "sid", "org_id"],
      });
      claim(payload, "sid");
      if (payload.client_id !== undefined && payload.client_id !== clientId)
        throw new Error("Unexpected WorkOS client");
      return {
        provider: "workos",
        expiresAt: payload.exp!,
        subject: claim(payload, "sub"),
        externalOrganisation: claim(payload, "org_id"),
      };
    });
  }

  if (options.entra) {
    const { tenantId, clientId, audience, scope } = options.entra;
    const issuer = `https://login.microsoftonline.com/${tenantId}/v2.0`;
    const discoveryUrl = `${issuer}/.well-known/openid-configuration`;
    let discovery: Promise<ReturnType<typeof createRemoteJWKSet>> | undefined;
    const keys = () =>
      (discovery ??= (async () => {
        const response = await fetchRemote(discoveryUrl, {
          headers: new Headers(),
          method: "GET",
          redirect: "manual",
          signal: AbortSignal.timeout(5000),
        });
        if (!response.ok) throw new Error("Entra discovery failed");
        const metadata = (await response.json()) as { issuer?: unknown; jwks_uri?: unknown };
        if (metadata.issuer !== issuer || typeof metadata.jwks_uri !== "string") {
          throw new Error("Unexpected Entra discovery metadata");
        }
        const jwks = new URL(metadata.jwks_uri);
        if (jwks.origin !== new URL(issuer).origin) throw new Error("Unexpected Entra JWKS origin");
        return createRemoteJWKSet(jwks, { [customFetch]: fetchRemote });
      })().catch((error: unknown) => {
        discovery = undefined;
        throw error;
      }));
    verifiers.set(issuer, async (token) => {
      const { payload } = await jwtVerify(token, await keys(), {
        issuer,
        audience,
        algorithms: ["RS256"],
        clockTolerance: externalClockTolerance,
        requiredClaims: ["exp", "iat", "ver", "oid", "tid", "azp", "scp"],
      });
      if (
        payload.ver !== "2.0" ||
        claim(payload, "tid").toLowerCase() !== tenantId ||
        claim(payload, "azp").toLowerCase() !== clientId ||
        !claim(payload, "scp").split(" ").includes(scope)
      ) {
        throw new Error("Unexpected Entra tenant, client or scope");
      }
      return {
        provider: "entra",
        expiresAt: payload.exp!,
        subject: claim(payload, "oid").toLowerCase(),
        externalOrganisation: tenantId,
        claims: { ...signInClaims(payload), guest: entraGuest(payload, tenantId) },
      };
    });
  }

  if (options.dev && devKey) {
    verifiers.set(devIssuer, async (token) => {
      const { payload } = await jwtVerify(token, devKey, {
        issuer: devIssuer,
        audience: devAudience,
        algorithms: ["HS256"],
        requiredClaims: ["exp", "iat", "sub", "org_id"],
      });
      return {
        provider: "dev",
        expiresAt: payload.exp!,
        subject: claim(payload, "sub"),
        externalOrganisation: claim(payload, "org_id"),
      };
    });
  }

  return async (token: string) => {
    if (!token || token.length > 16_384) return null;
    try {
      // The unverified issuer only selects a configured verifier. Signature,
      // expiration and provider claims are checked before identity is returned.
      // Agent and device tokens are never a provider's: their verifier needs the member's key.
      const issuer = decodeJwt(token).iss;
      const verify = issuer && !isKeyTokenIssuer(issuer) ? verifiers.get(issuer) : undefined;
      return verify ? await verify(token) : null;
    } catch {
      return null;
    }
  };
}
