import { afterAll, expect, test } from "bun:test";
import { exportJWK, generateKeyPair, SignJWT } from "jose";
import { jwtVerifiers, mintDevToken } from "../src/auth";
import { authProviders, config } from "../src/config";
import { validContent } from "../src/file-validation";

const databaseUrl = "postgresql://subspace@127.0.0.1/subspace";
const key = crypto.getRandomValues(new Uint8Array(32));
const { privateKey, publicKey } = await generateKeyPair("RS256");
const jwk = await exportJWK(publicKey);
const jwks = Bun.serve({
  hostname: "127.0.0.1",
  port: 0,
  fetch: () => Response.json({ keys: [{ ...jwk, kid: "test" }] }),
});
afterAll(() => jwks.stop(true));

test("the WorkOS client ID has the form the desktop also checks", () => {
  const env = { DATABASE_URL: databaseUrl };
  expect(
    config({ ...env, WORKOS_CLIENT_ID: "client_00000000000000000000000000" }).workos?.clientId,
  ).toBe("client_00000000000000000000000000");
  for (const value of ["", "client_", "client id", "client_a-b", "other_123"]) {
    expect(() => config({ ...env, WORKOS_CLIENT_ID: value })).toThrow("WORKOS_CLIENT_ID");
  }
});

test("the example environment's WorkOS placeholders pass the same check", async () => {
  const text = await Bun.file(new URL("../.env.example", import.meta.url)).text();
  const clientId = /^# WORKOS_CLIENT_ID=(.*)$/m.exec(text)?.[1] ?? "";
  expect(config({ DATABASE_URL: databaseUrl, WORKOS_CLIENT_ID: clientId }).workos?.clientId).toBe(
    clientId,
  );
  const jwksUrl = /^# WORKOS_JWKS_URL=(.*)$/m.exec(text)?.[1] ?? "";
  expect(jwksUrl.endsWith(`/${clientId}`)).toBe(true);
});

test("development sign-in requires explicit opt-in and a local listener", async () => {
  expect([...authProviders({})]).toEqual(["workos"]);
  const env = { DATABASE_URL: databaseUrl, AUTH_PROVIDERS: "dev", ENABLE_DEV_AUTH: "true" };
  expect(() => authProviders({ AUTH_PROVIDERS: "dev" })).toThrow("ENABLE_DEV_AUTH");
  expect(() => config({ ...env, NODE_ENV: "production" })).toThrow("outside production");
  expect(() => config({ ...env, HOST: "0.0.0.0" })).toThrow("loopback listener");
  const options = config(env);
  expect(
    await jwtVerifiers(
      config({ DATABASE_URL: databaseUrl, WORKOS_CLIENT_ID: "client_example" }),
      key,
    )(await mintDevToken(key, "tester@example.com", "example")),
  ).toBeNull();
  const verify = jwtVerifiers(options, key);
  const token = await mintDevToken(key, "tester@example.com", "example");
  const identity = await verify(token);
  expect(identity).toMatchObject({
    provider: "dev",
    subject: "tester@example.com",
    externalOrganisation: "example",
  });
  const otherKey = crypto.getRandomValues(new Uint8Array(32));
  expect(await jwtVerifiers(options, otherKey)(token)).toBeNull();
  expect(await jwtVerifiers(options, null)(token)).toBeNull();
});

test("WorkOS AuthKit verifies the documented session claims and tolerates brief clock skew", async () => {
  const clientId = "client_example";
  const issuer = "https://api.workos.com/";
  const env = {
    DATABASE_URL: databaseUrl,
    WORKOS_CLIENT_ID: clientId,
    WORKOS_JWKS_URL: new URL("/jwks", jwks.url).href,
  };
  const verify = jwtVerifiers(config(env), null);
  const sign = (
    changes: {
      client?: string;
      org?: string;
      issuer?: string;
      expires?: string;
      aud?: string;
      nbf?: string;
    } = {},
  ) => {
    const token = new SignJWT({
      sid: "session_example",
      org_id: changes.org ?? "org_example",
      role: "member",
      permissions: ["issues:read"],
      ...(changes.client ? { client_id: changes.client } : {}),
    })
      .setProtectedHeader({ alg: "RS256", kid: "test" })
      .setIssuer(changes.issuer ?? issuer)
      .setSubject("user_example")
      .setIssuedAt()
      .setExpirationTime(changes.expires ?? "1h");
    if (changes.aud) token.setAudience(changes.aud);
    if (changes.nbf) token.setNotBefore(changes.nbf);
    return token.sign(privateKey);
  };
  expect(await verify(await sign())).toMatchObject({
    provider: "workos",
    subject: "user_example",
    externalOrganisation: "org_example",
  });
  expect(await verify(await sign({ client: clientId }))).not.toBeNull();
  expect(await verify(await sign({ client: "client_other" }))).toBeNull();
  expect(await verify(await sign({ org: "" }))).toBeNull();
  expect(await verify(await sign({ issuer: "https://api.workos.com" }))).toBeNull();
  expect(await verify(await sign({ issuer: "https://example.com" }))).toBeNull();
  expect(await verify(await sign({ expires: "-2s" }))).not.toBeNull();
  expect(await verify(await sign({ nbf: "2s" }))).not.toBeNull();
  expect(await verify(await sign({ expires: "-1m" }))).toBeNull();
  expect(await verify(await sign({ nbf: "1m" }))).toBeNull();
  const token = await sign();
  const [header, claims, signature] = token.split(".");
  const altered = signature![0] === "a" ? "b" : "a";
  expect(await verify(`${header}.${claims}.${altered}${signature!.slice(1)}`)).toBeNull();

  const audience = "https://api.example.com";
  const scoped = jwtVerifiers(config({ ...env, WORKOS_AUDIENCE: audience }), null);
  expect(await scoped(token)).toBeNull();
  expect(await scoped(await sign({ aud: audience }))).not.toBeNull();
});

test("file admission rejects mismatched names, unsafe paths and invalid UTF-8", () => {
  expect(validContent("sample.txt", "text/plain", Buffer.from("Text\n"))).toBeTrue();
  expect(validContent("../sample.txt", "text/plain", Buffer.from("Text"))).toBeFalse();
  expect(validContent("sample.pdf", "text/plain", Buffer.from("Text"))).toBeFalse();
  expect(validContent("sample.txt", "text/plain", Buffer.from([0xff]))).toBeFalse();
  expect(validContent("sample.png", "image/png", Buffer.from("<svg></svg>"))).toBeFalse();
  expect(validContent("sample.zip", "application/zip", Buffer.from("PK\x03\x04"))).toBeFalse();
});

test("Entra discovery scopes access tokens to tenant, API audience and desktop client", async () => {
  const tenant = crypto.randomUUID();
  const client = crypto.randomUUID();
  const oid = crypto.randomUUID();
  const audience = crypto.randomUUID();
  const issuer = `https://login.microsoftonline.com/${tenant}/v2.0`;
  const options = config({
    DATABASE_URL: databaseUrl,
    AUTH_PROVIDERS: "entra",
    ENTRA_TENANT_ID: tenant,
    ENTRA_CLIENT_ID: client,
    ENTRA_AUDIENCE: audience,
  });
  const keyUrl = `${issuer}/keys`;
  const verify = jwtVerifiers(options, null, async (url) => {
    if (url === `${issuer}/.well-known/openid-configuration`)
      return Response.json({ issuer, jwks_uri: keyUrl });
    if (url === keyUrl) return Response.json({ keys: [{ ...jwk, kid: "test" }] });
    throw new Error("Unexpected provider URL");
  });
  const sign = (
    changes: {
      tid?: string;
      azp?: string;
      aud?: string;
      scp?: string;
      ver?: string;
      issuer?: string;
    } = {},
  ) =>
    new SignJWT({
      oid,
      ver: changes.ver ?? "2.0",
      tid: changes.tid ?? tenant,
      azp: changes.azp ?? client,
      scp: changes.scp ?? "API.Access",
    })
      .setProtectedHeader({ alg: "RS256", kid: "test" })
      .setIssuer(changes.issuer ?? issuer)
      .setAudience(changes.aud ?? audience)
      .setIssuedAt()
      .setExpirationTime("1h")
      .sign(privateKey);
  expect(await verify(await sign())).toMatchObject({
    provider: "entra",
    subject: oid,
    externalOrganisation: tenant,
  });
  expect(await verify(await sign({ tid: crypto.randomUUID() }))).toBeNull();
  expect(await verify(await sign({ azp: crypto.randomUUID() }))).toBeNull();
  expect(await verify(await sign({ aud: crypto.randomUUID() }))).toBeNull();
  expect(await verify(await sign({ ver: "1.0" }))).toBeNull();
  expect(await verify(await sign({ issuer: `${issuer}/` }))).toBeNull();
  expect(await verify(await sign({ scp: "Different.Scope" }))).toBeNull();
});
