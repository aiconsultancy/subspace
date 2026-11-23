/**
 * Test support: an isolated database on the local PostgreSQL named in TEST_DATABASE_URL, with
 * every migration applied, and the real API process on an ephemeral loopback port that trusts a
 * fictional Entra tenant whose keys only this test holds.
 */
import { fileURLToPath } from "node:url";
import { migrate } from "drizzle-orm/effect-postgres/migrator";
import { Effect, Exit, Layer, Scope } from "effect";
import { exportJWK, generateKeyPair, SignJWT, type JWTPayload } from "jose";
import pg from "pg";
import { server } from "../src/api";
import { type RemoteFetch } from "../src/auth";
import { config } from "../src/config";
import { Database, databaseLayer } from "../src/db";
import { type Storage } from "../src/storage";

export const testDatabase = process.env.TEST_DATABASE_URL;
const folder = fileURLToPath(new URL("../drizzle", import.meta.url));

/**
 * Parses the test database URL into explicit settings. It may carry no options, and its host must
 * be loopback, so no test can reach a shared or remote database.
 */
function localServer(raw: string) {
  const url = new URL(raw);
  if (!["postgres:", "postgresql:"].includes(url.protocol))
    throw new Error("TEST_DATABASE_URL must be a postgresql:// URL.");
  if (url.search || url.hash)
    throw new Error("TEST_DATABASE_URL must not carry connection options.");
  if (!["localhost", "127.0.0.1", "[::1]"].includes(url.hostname))
    throw new Error("TEST_DATABASE_URL must point to a local test database.");
  const host = url.hostname === "[::1]" ? "::1" : url.hostname;
  const port = Number(url.port || "5432");
  const user = decodeURIComponent(url.username);
  const password = decodeURIComponent(url.password);
  const settings = (database: string) => ({ host, port, user, password, database });
  const urlFor = (database: string) => {
    const clean = new URL(`${url.protocol}//${url.host}`);
    clean.username = url.username;
    clean.password = url.password;
    clean.pathname = `/${encodeURIComponent(database)}`;
    return clean.toString();
  };
  return { settings, urlFor, database: decodeURIComponent(url.pathname.slice(1)) || user };
}

export function applyMigrations(url: string, migrationsFolder = folder) {
  return Effect.runPromise(
    Effect.gen(function* () {
      const db = yield* Database;
      yield* migrate(db, { migrationsFolder });
    }).pipe(Effect.provide(databaseLayer(url, 2))),
  );
}

/** A fresh database with every migration applied, and a client connected to it. */
export async function isolatedDatabase(prefix: string, migrated = true) {
  const local = localServer(testDatabase!);
  const name = `${prefix}_${crypto.randomUUID().replaceAll("-", "")}`;
  const admin = new pg.Client(local.settings(local.database));
  await admin.connect();
  await admin.query(`CREATE DATABASE "${name}"`);
  const url = local.urlFor(name);
  if (migrated) await applyMigrations(url);
  const client = new pg.Client(local.settings(name));
  await client.connect();
  return {
    url,
    client,
    async drop() {
      await client.end();
      await admin.query(`DROP DATABASE IF EXISTS "${name}" WITH (FORCE)`);
      await admin.end();
    },
  };
}

/** A fictional Entra tenant: its configuration, a fetch serving its keys, and a token signer. */
export async function entraTenant() {
  const { privateKey, publicKey } = await generateKeyPair("RS256");
  const jwk = await exportJWK(publicKey);
  const tenant = crypto.randomUUID();
  const client = crypto.randomUUID();
  const audience = crypto.randomUUID();
  const issuer = `https://login.microsoftonline.com/${tenant}/v2.0`;
  const keyUrl = `${issuer}/keys`;
  const fetchRemote: RemoteFetch = async (url) => {
    if (url === `${issuer}/.well-known/openid-configuration`)
      return Response.json({ issuer, jwks_uri: keyUrl });
    if (url === keyUrl) return Response.json({ keys: [{ ...jwk, kid: "test" }] });
    throw new Error("Unexpected provider URL");
  };
  /** An access token for `oid`, with any claim replaced or added. */
  const sign = (oid: string, claims: JWTPayload = {}) =>
    new SignJWT({
      oid,
      ver: "2.0",
      tid: tenant,
      azp: client,
      scp: "API.Access",
      ...claims,
    })
      .setProtectedHeader({ alg: "RS256", kid: "test" })
      .setIssuer(issuer)
      .setAudience((claims.aud as string | undefined) ?? audience)
      .setIssuedAt()
      .setExpirationTime("1h")
      .sign(privateKey);
  return {
    tenant,
    sign,
    fetchRemote,
    env: {
      AUTH_PROVIDERS: "entra",
      ENTRA_TENANT_ID: tenant,
      ENTRA_CLIENT_ID: client,
      ENTRA_AUDIENCE: audience,
    },
  };
}

function freePort() {
  const probe = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response() });
  const { port } = probe;
  probe.stop(true);
  return port!;
}

/** The API process on an ephemeral loopback port, against `databaseUrl`. */
export async function startApi(
  databaseUrl: string,
  env: Record<string, string>,
  fetchRemote?: RemoteFetch,
) {
  const port = freePort();
  const options = config({ ...env, DATABASE_URL: databaseUrl, PORT: String(port) });
  const scope = await Effect.runPromise(Scope.make());
  await Effect.runPromise(
    Layer.buildWithScope(
      server(options, null, { store: {} as (typeof Storage)["Service"], fetchRemote }),
      scope,
    ),
  );
  const origin = `http://127.0.0.1:${port}`;
  return {
    origin,
    /** Calls the API with an optional bearer token and JSON body. */
    call(path: string, token?: string, method = "GET", body?: unknown, headers = {}) {
      return fetch(`${origin}${path}`, {
        method,
        headers: {
          ...(token ? { Authorization: `Bearer ${token}` } : {}),
          ...(body === undefined ? {} : { "Content-Type": "application/json" }),
          ...headers,
        },
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: AbortSignal.timeout(10_000),
      });
    },
    close: () => Effect.runPromise(Scope.close(scope, Exit.void)),
  };
}
