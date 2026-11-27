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
import { localDatabase } from "./local-database";

export const testDatabase = process.env.TEST_DATABASE_URL;
const folder = fileURLToPath(new URL("../drizzle", import.meta.url));

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
  const local = localDatabase(testDatabase!);
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
  /**
   * An access token for `oid`, with any claim replaced or added. `expires` sets the expiry, as
   * jose reads it, `issuedAt` the issue time in seconds since the epoch, and `key` signs with a
   * key the tenant does not publish.
   */
  const sign = (
    oid: string,
    claims: JWTPayload = {},
    options: { expires?: string | number; issuedAt?: number; key?: CryptoKey } = {},
  ) =>
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
      .setIssuedAt(options.issuedAt)
      .setExpirationTime(options.expires ?? "1h")
      .sign(options.key ?? privateKey);
  return {
    tenant,
    sign,
    fetchRemote,
    env: {
      AUTH_PROVIDERS: "entra",
      AUTO_REGISTER: "entra",
      ENTRA_TENANT_ID: tenant,
      ENTRA_CLIENT_ID: client,
      ENTRA_AUDIENCE: audience,
    },
  };
}

/** A WorkOS environment whose keys only this test holds, beside the Entra tenant. */
export async function workosEnvironment() {
  const { privateKey, publicKey } = await generateKeyPair("RS256");
  const jwk = { ...(await exportJWK(publicKey)), kid: "workos-test" };
  const jwksUrl = "https://workos.example.com/jwks";
  /** A WorkOS access token for `subject` in `organisation`, issued `age` seconds ago. */
  const sign = (subject: string, organisation: string, age = 0) => {
    const iat = Math.floor(Date.now() / 1000) - age;
    return new SignJWT({ sid: crypto.randomUUID(), org_id: organisation })
      .setProtectedHeader({ alg: "RS256", kid: "workos-test" })
      .setIssuer("https://api.workos.com/")
      .setSubject(subject)
      .setIssuedAt(iat)
      .setExpirationTime(iat + 3600)
      .sign(privateKey);
  };
  return { jwk, jwksUrl, sign };
}

/** Waits until `count` connections to the database wait for a lock, other than `client`'s. */
export async function lockWaiters(client: pg.Client, count: number) {
  for (let waited = 0; ; waited += 25) {
    const { rows } = await client.query(
      "SELECT count(*)::int AS n FROM pg_stat_activity WHERE datname = current_database() AND wait_event_type = 'Lock'",
    );
    if (rows[0].n >= count) return;
    if (waited > 5000) throw new Error(`Fewer than ${count} requests waited for a lock`);
    await Bun.sleep(25);
  }
}

/**
 * Starts `request` while another connection holds the organisation row lock that every guarded
 * write takes, runs `during` once the request waits for a lock, then releases it: so `during`
 * commits after the request signed in and before it reads its caller again under the lock.
 * `during` may start more requests and wait for them with `waiters`.
 */
export async function acrossOrganisationLock<T>(
  databaseUrl: string,
  organisationId: string,
  request: () => Promise<T>,
  during: (waiters: (count: number) => Promise<void>) => Promise<unknown>,
) {
  const barrier = new pg.Client({ connectionString: databaseUrl });
  await barrier.connect();
  let pending: Promise<T> | undefined;
  try {
    await barrier.query("BEGIN");
    await barrier.query("SELECT id FROM organisations WHERE id = $1 FOR NO KEY UPDATE", [
      organisationId,
    ]);
    pending = request();
    pending.catch(() => {});
    await lockWaiters(barrier, 1);
    await during((count) => lockWaiters(barrier, count));
  } finally {
    await barrier.query("COMMIT").catch(() => {});
    await barrier.end();
  }
  return pending;
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
  store = {} as (typeof Storage)["Service"],
) {
  const port = freePort();
  const options = config({ ...env, DATABASE_URL: databaseUrl, PORT: String(port) });
  const scope = await Effect.runPromise(Scope.make());
  await Effect.runPromise(
    Layer.buildWithScope(server(options, null, { store, fetchRemote }), scope),
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
