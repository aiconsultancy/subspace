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

const externalClockTolerance = 5;
const devIssuer = "subspace-dev";
const devAudience = "subspace-api";
export const devTokenLifetime = 28_800;

export type PrincipalIdentity = {
  provider: Provider;
  externalOrganisation: string;
  subject: string;
};

type VerifiedIdentity = PrincipalIdentity & { expiresAt: number };
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

export function jwtVerifiers(
  options: Config,
  devKey: Uint8Array | null,
  fetchRemote: NonNullable<
    NonNullable<Parameters<typeof createRemoteJWKSet>[1]>[typeof customFetch]
  > = fetch,
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
      const issuer = decodeJwt(token).iss;
      const verify = issuer ? verifiers.get(issuer) : undefined;
      return verify ? await verify(token) : null;
    } catch {
      return null;
    }
  };
}
