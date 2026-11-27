import { afterAll, beforeAll, describe, expect, setSystemTime, test } from "bun:test";
import { createHash, createPublicKey, X509Certificate } from "node:crypto";
import { Effect } from "effect";
import { OpenApi } from "effect/http-api";
import { jwtVerify } from "jose";
import pg from "pg";
import { parseCertificate } from "../src/agent-keys";
import { documentedApi } from "../src/api";
import { config, deviceTokenAudience } from "../src/config";
import { databaseLayer } from "../src/db";
import { problemTypes } from "../src/problems";
import { mintPairingInvite, pairingInvitePrefix, readPairingInvite } from "../src/pairing-invites";
import { bootstrapOwner, BootstrapRefused, deviceChoice } from "../src/pairing-owner";
import fixture from "../docs/changes/workstation-pairing/device-token-fixture.json";
import { agentKey, agentToken, deviceToken, type AgentKey } from "./certificates";
import { entraTenant, isolatedDatabase, startApi, testDatabase } from "./entra-stack";

/** The officers' audience, and the private name workstations reach the API by. */
const agentAudience = "https://api.example.com";
const deviceAudience = "https://subspace.example.internal";

describe("configuration", () => {
  test("pairing needs a device audience, which defaults to the agent audience", () => {
    const base = { DATABASE_URL: "postgresql://subspace@127.0.0.1/subspace" };
    expect(() => config({ ...base, AUTH_PROVIDERS: "pairing" })).toThrow(
      "Pairing needs DEVICE_TOKEN_AUDIENCE or AGENT_TOKEN_AUDIENCE",
    );
    expect(
      config({ ...base, AUTH_PROVIDERS: "pairing", AGENT_TOKEN_AUDIENCE: agentAudience })
        .deviceTokenAudience,
    ).toBe(agentAudience);
    expect(
      config({
        ...base,
        AUTH_PROVIDERS: "pairing",
        AGENT_TOKEN_AUDIENCE: agentAudience,
        DEVICE_TOKEN_AUDIENCE: deviceAudience,
      }).deviceTokenAudience,
    ).toBe(deviceAudience);
    // Without pairing in AUTH_PROVIDERS, no device is paired or accepted, whatever is set.
    expect(
      config({
        ...base,
        AUTH_PROVIDERS: "workos",
        WORKOS_CLIENT_ID: "client_example",
        DEVICE_TOKEN_AUDIENCE: deviceAudience,
      }).deviceTokenAudience,
    ).toBeNull();
    for (const value of [`${deviceAudience}/`, "https://Subspace.example.com", "ftp://x"]) {
      expect(() => deviceTokenAudience({ DEVICE_TOKEN_AUDIENCE: value })).toThrow(
        "DEVICE_TOKEN_AUDIENCE must be the API's public origin",
      );
    }
  });

  test("pairing needs an https device audience unless its host is loopback", () => {
    const base = {
      DATABASE_URL: "postgresql://subspace@127.0.0.1/subspace",
      AUTH_PROVIDERS: "pairing",
    };
    for (const audience of [
      "http://127.0.0.1:3000",
      "http://127.8.0.1",
      "http://localhost:3000",
      "http://[::1]:3000",
    ]) {
      expect(config({ ...base, DEVICE_TOKEN_AUDIENCE: audience }).deviceTokenAudience).toBe(
        audience,
      );
    }
    expect(() =>
      config({ ...base, DEVICE_TOKEN_AUDIENCE: "http://subspace.example.internal" }),
    ).toThrow(
      "Pairing needs https: DEVICE_TOKEN_AUDIENCE must be an https origin unless its host is loopback",
    );
    // The agent audience, when it stands in for the device audience, is held to the same rule.
    expect(() => config({ ...base, AGENT_TOKEN_AUDIENCE: "http://api.example.com" })).toThrow(
      "Pairing needs https: AGENT_TOKEN_AUDIENCE, the device token audience",
    );
    expect(() => config({ ...base, AGENT_TOKEN_AUDIENCE: "http://127.example.com" })).toThrow(
      "Pairing needs https",
    );
    // An https device audience lets the agent audience stay http, and pairing off needs neither.
    expect(
      config({
        ...base,
        AGENT_TOKEN_AUDIENCE: "http://api.example.com",
        DEVICE_TOKEN_AUDIENCE: deviceAudience,
      }).deviceTokenAudience,
    ).toBe(deviceAudience);
    expect(
      config({
        ...base,
        AUTH_PROVIDERS: "workos",
        WORKOS_CLIENT_ID: "client_example",
        DEVICE_TOKEN_AUDIENCE: "http://subspace.example.internal",
      }).deviceTokenAudience,
    ).toBeNull();
  });
});

test("an invite is versioned, single-line base64url, and only its own shape is read", () => {
  const id = crypto.randomUUID();
  const { token } = mintPairingInvite(id);
  expect(token.startsWith(pairingInvitePrefix)).toBe(true);
  expect(token).toMatch(/^spair1\.[A-Za-z0-9_-]{64}$/u);
  expect(readPairingInvite(token)?.id).toBe(id);
  expect(readPairingInvite(token.replace("spair1.", "senr1."))).toBeUndefined();
  expect(readPairingInvite(`${token}=`)).toBeUndefined();
  expect(readPairingInvite(`${token}\n`)).toBeUndefined();
});

test("the pairing routes are documented, without a bearer token", () => {
  const spec = OpenApi.fromApi(documentedApi) as {
    paths: Record<string, Record<string, { security?: unknown[] }>>;
  };
  expect(Object.keys(spec.paths["/api/v1/pairing"]!)).toEqual(["get"]);
  expect(Object.keys(spec.paths["/api/v1/pairing/redeem"]!)).toEqual(["post"]);
  expect(spec.paths["/api/v1/pairing"]!.get!.security ?? []).toEqual([]);
  expect(spec.paths["/api/v1/pairing/redeem"]!.post!.security ?? []).toEqual([]);
});

test("the published fixture is a valid device token for its certificate", async () => {
  const der = new X509Certificate(fixture.certificate).raw;
  expect(createHash("sha256").update(der).digest("hex")).toBe(fixture.fingerprint);
  const parsed = await parseCertificate(fixture.certificate);
  expect(parsed.ok && parsed.certificate).toMatchObject({
    fingerprint: fixture.fingerprint,
    algorithm: "EdDSA",
  });
  const { payload, protectedHeader } = await jwtVerify(
    fixture.token,
    createPublicKey(fixture.certificate),
    {
      issuer: "subspace-device",
      audience: fixture.claims.aud,
      typ: "subspace-device+jwt",
      algorithms: ["EdDSA"],
      currentDate: new Date(fixture.verifyAt * 1000),
    },
  );
  expect(protectedHeader).toEqual(fixture.header);
  expect(payload).toEqual(fixture.claims);
  // The private key is the certificate's.
  const { privateKeyJwk } = fixture;
  expect(privateKeyJwk.x).toBe(
    createPublicKey(fixture.certificate).export({ format: "jwk" }).x as string,
  );
});

describe.skipIf(!testDatabase)("workstation pairing on the main listener", () => {
  let database: Awaited<ReturnType<typeof isolatedDatabase>>;
  let tenant: Awaited<ReturnType<typeof entraTenant>>;
  let api: Awaited<ReturnType<typeof startApi>>;
  let unpairing: Awaited<ReturnType<typeof startApi>>;
  const organisation = "example";
  const logged: string[] = [];
  const restore: (() => void)[] = [];

  beforeAll(async () => {
    // Everything the API logs in this file, to check that no invite reaches a log line.
    for (const level of ["log", "info", "debug", "warn", "error", "trace"] as const) {
      const original = console[level];
      console[level] = (...args: unknown[]) => {
        logged.push(
          args.map((arg) => (typeof arg === "string" ? arg : Bun.inspect(arg))).join(" "),
        );
        original.apply(console, args);
      };
      restore.push(() => {
        console[level] = original;
      });
    }
    database = await isolatedDatabase("pairing");
    tenant = await entraTenant();
    const env = { ...tenant.env, NODE_ENV: "production", AGENT_TOKEN_AUDIENCE: agentAudience };
    api = await startApi(
      database.url,
      {
        ...env,
        AUTH_PROVIDERS: "entra,pairing",
        DEVICE_TOKEN_AUDIENCE: deviceAudience,
        IRC_CREDENTIAL_SECRET: "an example credential secret of at least 32 bytes",
        IRC_AUTH_SERVERS: "irc.example.com:6697",
      },
      tenant.fetchRemote,
    );
    unpairing = await startApi(database.url, env, tenant.fetchRemote);
  });
  afterAll(async () => {
    await api?.close();
    await unpairing?.close();
    await database?.drop();
    for (const undo of restore) undo();
  });

  /** The bootstrap command, its device flags mapped as the command maps them. */
  const bootstrap = async (
    options: {
      revokeDevices?: boolean;
      reactivate?: boolean;
      keepDevices?: boolean;
      name?: string;
    } = {},
  ) =>
    Effect.runPromise(
      bootstrapOwner({
        externalOrganisation: organisation,
        displayName: options.name ?? "Tamsin Pryor",
        email: "tamsin@example.com",
        lifetime: 3600,
        reactivate: options.reactivate,
        devices: deviceChoice(options),
        reserved: new Set(),
      }).pipe(Effect.provide(databaseLayer(database.url, 2))),
    );
  /** The IDs of a member's device credentials that are not revoked, oldest first. */
  const unrevokedOf = async (memberId: string) =>
    (
      await database.client.query(
        "SELECT id FROM agent_credentials WHERE user_id = $1 AND revoked_at IS NULL ORDER BY created_at, id",
        [memberId],
      )
    ).rows.map((row: { id: string }) => row.id);
  /** What the latest reactivation of a member recorded. */
  const lastReactivation = async (memberId: string) =>
    (
      await database.client.query(
        "SELECT changes FROM audit_logs WHERE action = 'memberReactivated' AND entity_id = $1 ORDER BY occurred_at DESC LIMIT 1",
        [memberId],
      )
    ).rows[0]?.changes;
  const redeem = (invite: string, key: AgentKey, extra: Record<string, unknown> = {}) =>
    api.call("/api/v1/pairing/redeem", undefined, "POST", {
      invite,
      certificate: key.certificate,
      ...extra,
    });
  /** A fresh invite of the owner's, redeemed by a new device. */
  const pairDevice = async (kind: "ed25519" | "p256" = "ed25519") => {
    const { invite, owner } = await bootstrap();
    const key = agentKey(kind);
    const response = await redeem(invite.token, key, { label: "Workstation" });
    expect(response.status).toBe(201);
    expect(response.headers.get("cache-control")).toBe("no-store");
    return { key, owner, invite, body: await response.json() };
  };
  const me = async (token: string) => {
    const response = await api.call("/api/v1/me", token);
    return { status: response.status, body: await response.json() };
  };
  const credentialsOf = async (inviteId: string) =>
    Number(
      (
        await database.client.query(
          "SELECT count(*) FROM agent_credentials WHERE pairing_invite_id = $1",
          [inviteId],
        )
      ).rows[0].count,
    );
  const unusable = {
    type: problemTypes.pairingInviteUnusable,
    status: 401,
    detail: "This pairing invite cannot be used. Ask for a new invite.",
  };

  test("the capability probe needs no token and says whether pairing is on", async () => {
    const on = await api.call("/api/v1/pairing");
    expect(on.status).toBe(200);
    expect(await on.json()).toEqual({
      enabled: true,
      inviteVersion: "spair1",
      redeemPath: "/api/v1/pairing/redeem",
      deviceToken: {
        type: "subspace-device+jwt",
        issuer: "subspace-device",
        audience: deviceAudience,
        algorithms: ["EdDSA", "ES256"],
        maxLifetimeSeconds: 300,
        clockToleranceSeconds: 5,
      },
    });
    // The audience is configured: a request's Host header never changes it.
    const spoofed = await api.call("/api/v1/pairing", undefined, "GET", undefined, {
      Host: "attacker.example.com",
    });
    expect((await spoofed.json()).deviceToken.audience).toBe(deviceAudience);

    const off = await unpairing.call("/api/v1/pairing");
    expect(await off.json()).toMatchObject({ enabled: false, deviceToken: null });
    const refused = await unpairing.call("/api/v1/pairing/redeem", undefined, "POST", {
      invite: `${pairingInvitePrefix}${"A".repeat(64)}`,
      certificate: agentKey().certificate,
    });
    expect(refused.status).toBe(404);
    expect((await refused.json()).type).toBe(problemTypes.pairingUnavailable);
  });

  test("bootstrap creates the organisation and owner on a fresh database, and a rerun reuses them", async () => {
    await expect(
      Effect.runPromise(
        bootstrapOwner({
          externalOrganisation: "nameless",
          lifetime: 3600,
          reserved: new Set(),
        }).pipe(Effect.provide(databaseLayer(database.url, 2))),
      ),
    ).rejects.toBeInstanceOf(BootstrapRefused);
    await expect(
      Effect.runPromise(
        bootstrapOwner({
          externalOrganisation: "nameless",
          displayName: "Not an existing owner",
          lifetime: 3600,
          reactivate: true,
          reserved: new Set(),
        }).pipe(Effect.provide(databaseLayer(database.url, 2))),
      ),
    ).rejects.toThrow("There is no owner to reactivate");

    const first = await bootstrap();
    expect(first).toMatchObject({
      organisationCreated: true,
      ownerCreated: true,
      revokedInvites: 0,
      owner: { displayName: "Tamsin Pryor", username: "tamsin" },
    });
    const [owner] = (
      await database.client.query(
        "SELECT u.subject, u.role, u.agent_rank, o.provider FROM users u JOIN organisations o ON o.id = u.organisation_id WHERE u.id = $1",
        [first.owner.id],
      )
    ).rows;
    expect(owner).toEqual({
      subject: `subspace-member:${first.owner.id}`,
      role: "admin",
      agent_rank: null,
      provider: "pairing",
    });
    const [stored] = (
      await database.client.query("SELECT token_hash FROM pairing_invites WHERE id = $1", [
        first.invite.id,
      ])
    ).rows;
    expect(stored.token_hash).toBe(createHash("sha256").update(first.invite.token).digest("hex"));

    // The rerun, even with another name, finds the same owner and replaces the pending invite.
    const second = await bootstrap({ name: "Someone Else" });
    expect(second).toMatchObject({
      organisationId: first.organisationId,
      organisationCreated: false,
      ownerCreated: false,
      revokedInvites: 1,
      owner: { id: first.owner.id, displayName: "Tamsin Pryor" },
    });
    expect(await (await redeem(first.invite.token, agentKey())).json()).toMatchObject(unusable);
    const response = await redeem(second.invite.token, agentKey());
    expect(response.status).toBe(201);
  });

  test("a redeemed device signs a request that returns the owner, on REST and MCP", async () => {
    const { key, owner, body } = await pairDevice();
    expect(body).toMatchObject({
      memberId: owner.id,
      fingerprint: key.fingerprint,
      scopes: ["api", "irc"],
      deviceToken: { audience: deviceAudience },
    });
    const signedIn = await me(await deviceToken(key, deviceAudience));
    expect(signedIn.status).toBe(200);
    expect(signedIn.body).toMatchObject({
      userId: owner.id,
      kind: "human",
      displayName: "Tamsin Pryor",
      username: "tamsin",
      isAdmin: true,
    });
    const whoami = await api.call("/mcp", await deviceToken(key, deviceAudience), "POST", {
      jsonrpc: "2.0",
      id: 1,
      method: "tools/call",
      params: { name: "subspace_whoami", arguments: {} },
    });
    expect(whoami.status).toBe(200);
    expect((await whoami.json()).result.structuredContent).toEqual(signedIn.body);
    // A P-256 device works the same way.
    const p256 = await pairDevice("p256");
    expect((await me(await deviceToken(p256.key, deviceAudience))).body.userId).toBe(owner.id);
  });

  test("a device's update stream ends when its token expires, for a reconnect with a fresh one", async () => {
    const { key } = await pairDevice();
    const created = await api.call(
      "/api/v1/projects",
      await deviceToken(key, deviceAudience),
      "POST",
      {
        key: "PAIR",
        name: "Paired project",
      },
    );
    expect(created.status).toBe(201);
    // A token with two seconds left: the stream ends then, not three minutes later.
    const now = Math.floor(Date.now() / 1000);
    const token = await deviceToken(key, deviceAudience, { issuedAt: now - 298, lifetime: 300 });
    const opened = performance.now();
    const stream = await api.call("/api/v1/updates?project=PAIR", token);
    expect(stream.status).toBe(200);
    const reader = stream.body!.getReader();
    const decoder = new TextDecoder();
    let text = "";
    for (let chunk = await reader.read(); !chunk.done; chunk = await reader.read())
      text += decoder.decode(chunk.value);
    expect(text).toContain("event: ready");
    expect(performance.now() - opened).toBeLessThan(5000);
  });

  test("a used invite is refused, and a retry with the same certificate registers nothing", async () => {
    const { key, invite } = await pairDevice();
    for (const retry of [key, agentKey()]) {
      const response = await redeem(invite.token, retry);
      expect(response.status).toBe(401);
      expect(await response.json()).toMatchObject(unusable);
    }
    expect(await credentialsOf(invite.id)).toBe(1);
    // A lost answer is recovered with a signed request: the redemption committed.
    expect((await me(await deviceToken(key, deviceAudience))).status).toBe(200);
  });

  test("an expired, revoked, unknown or malformed invite is refused alike", async () => {
    const expired = await bootstrap();
    await database.client.query(
      "UPDATE pairing_invites SET expires_at = now() - interval '1 second' WHERE id = $1",
      [expired.invite.id],
    );
    const replaced = await bootstrap();
    const current = await bootstrap();
    const unknown = `${pairingInvitePrefix}${Buffer.alloc(48, 7).toString("base64url")}`;
    const forged = current.invite.token.slice(0, -4) + "AAAA";
    for (const invite of [
      expired.invite.token,
      replaced.invite.token,
      unknown,
      forged,
      "spair1.short",
      `senr1.${current.invite.token.slice(7)}`,
    ]) {
      const response = await redeem(invite, agentKey());
      expect(response.status).toBe(401);
      const text = await response.text();
      expect(JSON.parse(text)).toMatchObject(unusable);
      expect(text).not.toContain(invite);
    }
    expect(await credentialsOf(expired.invite.id)).toBe(0);
    // The current invite still works: refusals of others did not touch it.
    expect((await redeem(current.invite.token, agentKey())).status).toBe(201);
  });

  test("expiry is the database's answer, whatever this process's clock says", async () => {
    const lapsed = await bootstrap();
    await database.client.query(
      "UPDATE pairing_invites SET expires_at = now() - interval '1 second' WHERE id = $1",
      [lapsed.invite.id],
    );
    // A replica whose clock is an hour slow still refuses an invite the database calls expired.
    setSystemTime(new Date(Date.now() - 3600_000));
    try {
      const refused = await redeem(lapsed.invite.token, agentKey());
      expect(refused.status).toBe(401);
      expect(await refused.json()).toMatchObject(unusable);
    } finally {
      setSystemTime();
    }
    expect(await credentialsOf(lapsed.invite.id)).toBe(0);
    // One whose clock is an hour fast still pairs with an invite that has a minute left.
    const current = await bootstrap();
    await database.client.query(
      "UPDATE pairing_invites SET expires_at = now() + interval '1 minute' WHERE id = $1",
      [current.invite.id],
    );
    setSystemTime(new Date(Date.now() + 3600_000));
    try {
      expect((await redeem(current.invite.token, agentKey())).status).toBe(201);
    } finally {
      setSystemTime();
    }
  });

  test("a host clock ahead of the database's still leaves only the newest invite usable", async () => {
    const first = await bootstrap();
    // Two hours ahead: past the first invite's expiry by this process's clock, not the database's.
    setSystemTime(new Date(Date.now() + 2 * 3600_000));
    const second = await (async () => {
      try {
        return await bootstrap();
      } finally {
        setSystemTime();
      }
    })();
    expect(second.revokedInvites).toBe(1);
    // The new invite lasts its lifetime by the database's clock, which redemption judges it by.
    const [row] = (
      await database.client.query(
        "SELECT extract(epoch FROM expires_at - clock_timestamp())::float AS left FROM pairing_invites WHERE id = $1",
        [second.invite.id],
      )
    ).rows as { left: number }[];
    expect(row!.left).toBeGreaterThan(3600 - 60);
    expect(row!.left).toBeLessThanOrEqual(3600);
    const refused = await redeem(first.invite.token, agentKey());
    expect(refused.status).toBe(401);
    expect(await refused.json()).toMatchObject(unusable);
    expect((await redeem(second.invite.token, agentKey())).status).toBe(201);
  });

  test("malformed requests and conflicting certificates have stable problem types", async () => {
    const { invite } = await bootstrap();
    for (const body of [
      { certificate: agentKey().certificate },
      { invite: invite.token },
      { invite: invite.token, certificate: agentKey().certificate, extra: true },
      { invite: invite.token, certificate: agentKey("p384").certificate },
      { invite: invite.token, certificate: "not a certificate" },
    ]) {
      const response = await api.call("/api/v1/pairing/redeem", undefined, "POST", body);
      expect(response.status).toBe(400);
      const text = await response.text();
      expect(JSON.parse(text).type).toBe(problemTypes.invalidRequest);
      expect(text).not.toContain(invite.token);
    }
    // A certificate already registered, as another device or an agent, is a conflict.
    const paired = await pairDevice();
    const next = await bootstrap();
    const conflict = await redeem(next.invite.token, paired.key);
    expect(conflict.status).toBe(409);
    expect((await conflict.json()).type).toBe(problemTypes.credentialConflict);
    expect(await credentialsOf(next.invite.id)).toBe(0);
    // The invite is not spent by a refused redemption.
    expect((await redeem(next.invite.token, agentKey())).status).toBe(201);
  });

  test("a missing or unsupported content type, or an oversized invite, is an invalid request", async () => {
    const { invite } = await bootstrap();
    const key = agentKey();
    const certificate = key.certificate;
    const oversized = invite.token.padEnd(257, "A");
    const json = (token: string) => JSON.stringify({ invite: token, certificate });
    const send = (body: BodyInit, contentType?: string) =>
      fetch(`${api.origin}/api/v1/pairing/redeem`, {
        method: "POST",
        headers: contentType === undefined ? {} : { "Content-Type": contentType },
        body,
        signal: AbortSignal.timeout(10_000),
      });
    for (const request of [
      // Bytes, so that fetch adds no Content-Type of its own.
      () => send(new TextEncoder().encode(json(invite.token))),
      () => send(json(invite.token), ""),
      () => send(json(invite.token), "text/plain"),
      () =>
        send(
          new URLSearchParams({ invite: invite.token, certificate }).toString(),
          "application/x-www-form-urlencoded",
        ),
      () => send(json(oversized), "application/json"),
    ]) {
      const response = await request();
      expect(response.status).toBe(400);
      expect(response.headers.get("content-type")).toStartWith("application/problem+json");
      const text = await response.text();
      expect(JSON.parse(text)).toMatchObject({ status: 400, type: problemTypes.invalidRequest });
      expect(text).not.toContain(invite.token);
    }
    expect(await credentialsOf(invite.id)).toBe(0);
    // JSON with parameters, or in another case, is still JSON.
    expect((await send(json(invite.token), "application/json; charset=utf-8")).status).toBe(201);
    const next = await bootstrap();
    const upper = JSON.stringify({
      invite: next.invite.token,
      certificate: agentKey().certificate,
    });
    expect((await send(upper, "Application/JSON")).status).toBe(201);
  });

  test("concurrent redemptions of one invite register one device", async () => {
    const { invite } = await bootstrap();
    const keys = Array.from({ length: 8 }, () => agentKey());
    const responses = await Promise.all(keys.map((key) => redeem(invite.token, key)));
    const statuses = responses.map((response) => response.status).sort();
    expect(statuses).toEqual([201, 401, 401, 401, 401, 401, 401, 401]);
    expect(await credentialsOf(invite.id)).toBe(1);
    const winner = keys[responses.findIndex((response) => response.status === 201)]!;
    expect((await me(await deviceToken(winner, deviceAudience))).status).toBe(200);
  });

  test("a wrong secret for a real invite takes no lock and leaves the invite usable", async () => {
    const { invite, organisationId } = await bootstrap();
    // The same row ID with another secret, as from someone who saw the ID but not the invite.
    const forged = mintPairingInvite(invite.id).token;
    expect(forged).not.toBe(invite.token);
    // Hold the organisation's and the invite's row locks, as a real write would.
    await database.client.query("BEGIN");
    try {
      await database.client.query("SELECT 1 FROM organisations WHERE id = $1 FOR UPDATE", [
        organisationId,
      ]);
      await database.client.query("SELECT 1 FROM pairing_invites WHERE id = $1 FOR UPDATE", [
        invite.id,
      ]);
      const refused = await Promise.race([
        redeem(forged, agentKey()),
        Bun.sleep(3000).then(() => "waited for a lock" as const),
      ]);
      expect(refused).not.toBe("waited for a lock");
      expect(await (refused as Response).json()).toMatchObject(unusable);
    } finally {
      await database.client.query("ROLLBACK");
    }
    expect(await credentialsOf(invite.id)).toBe(0);
    expect((await redeem(invite.token, agentKey())).status).toBe(201);
  });

  test("a replayed device token is refused", async () => {
    const { key } = await pairDevice();
    const token = await deviceToken(key, deviceAudience);
    expect((await me(token)).status).toBe(200);
    expect((await me(token)).status).toBe(401);
  });

  test("a device token is bound to the device audience, its own kind and its lifetime", async () => {
    const { key } = await pairDevice();
    for (const token of [
      // The officers' audience, or one taken from the request.
      await deviceToken(key, agentAudience),
      await deviceToken(key, api.origin),
      // An agent token from a person's device, or a device token naming the agent issuer.
      await agentToken(key, deviceAudience),
      await deviceToken(key, deviceAudience, { claims: { iss: "subspace-agent" } }),
      await deviceToken(key, deviceAudience, { lifetime: 301 }),
      await deviceToken(key, deviceAudience, {
        issuedAt: Math.floor(Date.now() / 1000) - 400,
      }),
    ]) {
      const response = await me(token);
      expect(response.status).toBe(401);
      expect(response.body.detail).toBe("The bearer access token is invalid or expired.");
    }
  });

  test("a device token cannot sign in an agent", async () => {
    // An administrator's agent with a key: an agent token works, a device token does not.
    const admin = await tenant.sign(crypto.randomUUID(), {
      given_name: "Kerensa",
      roles: ["Subspace.Admin"],
    });
    const signedIn = await api.call("/api/v1/me", admin);
    expect(signedIn.status).toBe(200);
    const registrar = ((await signedIn.json()) as { userId: string }).userId;
    const [{ organisation_id: orgId }] = (
      await database.client.query("SELECT organisation_id FROM users WHERE id = $1", [registrar])
    ).rows;
    const [{ id: agentId }] = (
      await database.client.query(
        `INSERT INTO users (organisation_id, subject, display_name, agent_rank, agent_fleet, agent_model, registered_by_id)
         VALUES ($1, $2, 'Keyed', 'captain', 'alpha', 'example-model', $3) RETURNING id`,
        [orgId, crypto.randomUUID(), registrar],
      )
    ).rows;
    const key = agentKey();
    const parsed = await parseCertificate(key.certificate);
    if (!parsed.ok) throw new Error("The test certificate did not parse.");
    await database.client.query(
      `INSERT INTO agent_credentials (organisation_id, user_id, fingerprint, algorithm, certificate, public_key, scopes)
       VALUES ($1, $2, $3, $4, $5, $6, '{api}')`,
      [
        orgId,
        agentId,
        key.fingerprint,
        parsed.certificate.algorithm,
        parsed.certificate.pem,
        parsed.certificate.publicKey,
      ],
    );
    expect((await me(await agentToken(key, agentAudience))).body.userId).toBe(agentId);
    expect((await me(await deviceToken(key, deviceAudience))).status).toBe(401);
  });

  test("an unpaired caller cannot claim a member through a header or a forged token", async () => {
    const { key, owner } = await pairDevice();
    const stranger = agentKey();
    for (const headers of [
      { "X-Subspace-Agent": owner.id },
      { "X-Subspace-Agent": "tamsin" },
      { "X-Subspace-Member": owner.id },
    ]) {
      const response = await api.call("/api/v1/me", undefined, "GET", undefined, headers);
      expect(response.status).toBe(401);
    }
    const unsigned = [
      Buffer.from(
        JSON.stringify({ alg: "none", typ: "subspace-device+jwt", kid: key.fingerprint }),
      ).toString("base64url"),
      Buffer.from(
        JSON.stringify({
          iss: "subspace-device",
          sub: key.fingerprint,
          aud: deviceAudience,
          iat: Math.floor(Date.now() / 1000),
          exp: Math.floor(Date.now() / 1000) + 60,
          jti: crypto.randomUUID(),
        }),
      ).toString("base64url"),
      "",
    ].join(".");
    for (const token of [
      // Another key claiming the owner's device fingerprint.
      await deviceToken(key, deviceAudience, { signWith: stranger.privateKey }),
      // An unregistered key naming itself.
      await deviceToken(stranger, deviceAudience),
      unsigned,
    ]) {
      expect((await me(token)).status).toBe(401);
    }
  });

  test("the agent credential routes cannot touch a device", async () => {
    const { key, owner, body } = await pairDevice();
    const revoke = await api.call(
      `/api/v1/agents/${owner.id}/credentials/${body.credentialId}`,
      await deviceToken(key, deviceAudience),
      "DELETE",
    );
    expect(revoke.status).toBe(404);
    expect((await me(await deviceToken(key, deviceAudience))).status).toBe(200);
  });

  test("a deactivated owner is refused with 403", async () => {
    const { key, owner } = await pairDevice();
    await database.client.query("UPDATE users SET active = false WHERE id = $1", [owner.id]);
    try {
      const response = await me(await deviceToken(key, deviceAudience));
      expect(response.status).toBe(403);
      expect(response.body.detail).toBe("An active organisation membership is required.");
      await expect(bootstrap()).rejects.toThrow("Rerun with --reactivate");
    } finally {
      await database.client.query("UPDATE users SET active = true WHERE id = $1", [owner.id]);
    }
  });

  test("host root can explicitly reactivate the same owner and replace their devices", async () => {
    const { key, owner } = await pairDevice();
    const devices = await unrevokedOf(owner.id);
    await database.client.query("UPDATE users SET active = false WHERE id = $1", [owner.id]);
    try {
      const restored = await bootstrap({ reactivate: true, revokeDevices: true, name: "Ignored" });
      expect(restored).toMatchObject({
        owner,
        ownerCreated: false,
        ownerReactivated: true,
        revokedDevices: devices.length,
        keptDevices: 0,
      });
      expect(await lastReactivation(owner.id)).toEqual({
        active: true,
        devices: "revoked",
        deviceCount: devices.length,
        deviceCredentialIds: devices,
      });
      expect((await me(await deviceToken(key, deviceAudience))).status).toBe(401);
      const replacement = agentKey();
      expect((await redeem(restored.invite.token, replacement)).status).toBe(201);
      expect((await me(await deviceToken(replacement, deviceAudience))).body.userId).toBe(owner.id);
      const { rows } = await database.client.query(
        `SELECT source, "actorType" AS actor_type FROM audit_logs WHERE action = 'memberReactivated' AND entity_id = $1`,
        [owner.id],
      );
      expect(rows).toEqual([{ source: "cli", actor_type: "integration" }]);
    } finally {
      await database.client.query("UPDATE users SET active = true WHERE id = $1", [owner.id]);
    }
  });

  test("reactivating an owner whose devices were never revoked needs an explicit choice", async () => {
    const { key, owner } = await pairDevice();
    const devices = await unrevokedOf(owner.id);
    expect(devices.length).toBeGreaterThan(0);
    await database.client.query("UPDATE users SET active = false WHERE id = $1", [owner.id]);
    try {
      await expect(bootstrap({ reactivate: true })).rejects.toThrow(
        `has ${devices.length} device credential(s) that would sign in again`,
      );
      await expect(
        bootstrap({ reactivate: true, keepDevices: true, revokeDevices: true }),
      ).rejects.toThrow("not both");
      await expect(bootstrap({ keepDevices: true })).rejects.toThrow("only with --reactivate");
      // A refusal changes nothing: the owner stays deactivated and keeps their devices.
      expect((await me(await deviceToken(key, deviceAudience))).status).toBe(403);
      expect(await unrevokedOf(owner.id)).toEqual(devices);

      const restored = await bootstrap({ reactivate: true, keepDevices: true });
      expect(restored).toMatchObject({ ownerReactivated: true, revokedDevices: 0 });
      expect(restored.keptDevices).toBe(devices.length);
      expect((await me(await deviceToken(key, deviceAudience))).status).toBe(200);
      expect(await unrevokedOf(owner.id)).toEqual(devices);
      expect(await lastReactivation(owner.id)).toEqual({
        active: true,
        devices: "kept",
        deviceCount: devices.length,
        deviceCredentialIds: devices,
      });
    } finally {
      await database.client.query("UPDATE users SET active = true WHERE id = $1", [owner.id]);
    }
  });

  test("reactivating an owner with no devices left needs no device flag", async () => {
    const { owner } = await bootstrap({ revokeDevices: true });
    expect(await unrevokedOf(owner.id)).toEqual([]);
    await database.client.query("UPDATE users SET active = false WHERE id = $1", [owner.id]);
    try {
      const restored = await bootstrap({ reactivate: true });
      expect(restored).toMatchObject({ ownerReactivated: true, revokedDevices: 0, keptDevices: 0 });
      expect(await lastReactivation(owner.id)).toEqual({
        active: true,
        devices: "none",
        deviceCount: 0,
        deviceCredentialIds: [],
      });
      const replacement = agentKey();
      expect((await redeem(restored.invite.token, replacement)).status).toBe(201);
      expect((await me(await deviceToken(replacement, deviceAudience))).status).toBe(200);
    } finally {
      await database.client.query("UPDATE users SET active = true WHERE id = $1", [owner.id]);
    }
  });

  test("bootstrap locks the owner, so a concurrent deactivation waits for its invite", async () => {
    const { owner, invite } = await bootstrap();
    const other = new pg.Client({ connectionString: database.url });
    await other.connect();
    let pending: ReturnType<typeof bootstrap> | undefined;
    await database.client.query("BEGIN");
    try {
      // Hold the owner's pending invite, which bootstrap revokes after it has read the owner.
      await database.client.query("SELECT 1 FROM pairing_invites WHERE id = $1 FOR UPDATE", [
        invite.id,
      ]);
      pending = bootstrap();
      pending.catch(() => {});
      for (let waited = 0; ; waited += 50) {
        const { rows } = await other.query(
          "SELECT count(*)::int AS n FROM pg_stat_activity WHERE datname = current_database() AND wait_event_type = 'Lock'",
        );
        if (rows[0].n > 0) break;
        if (waited > 5000) throw new Error("bootstrap never waited for the invite");
        await Bun.sleep(50);
      }
      // Bootstrap has read the owner and holds their row: a deactivation must wait for it.
      await other.query("SET lock_timeout = '300ms'");
      await expect(
        other.query("UPDATE users SET active = false WHERE id = $1", [owner.id]),
      ).rejects.toThrow("lock timeout");
    } finally {
      await database.client.query("ROLLBACK");
      await database.client.query("UPDATE users SET active = true WHERE id = $1", [owner.id]);
      await other.end();
    }
    const issued = await pending!;
    expect((await redeem(issued.invite.token, agentKey())).status).toBe(201);
  });

  test("a revoked device is refused on its next request; the owner's other devices keep working", async () => {
    const first = await pairDevice();
    const second = await pairDevice();
    expect((await me(await deviceToken(first.key, deviceAudience))).status).toBe(200);
    await database.client.query("UPDATE agent_credentials SET revoked_at = now() WHERE id = $1", [
      first.body.credentialId,
    ]);
    expect((await me(await deviceToken(first.key, deviceAudience))).status).toBe(401);
    expect((await me(await deviceToken(second.key, deviceAudience))).status).toBe(200);
  });

  test("bootstrap recovers a lost workstation: it revokes every device and issues a working invite", async () => {
    const lost = await pairDevice();
    const recovery = await bootstrap({ revokeDevices: true });
    expect(recovery.revokedDevices).toBeGreaterThan(0);
    expect((await me(await deviceToken(lost.key, deviceAudience))).status).toBe(401);
    const [{ count }] = (
      await database.client.query(
        "SELECT count(*) FROM agent_credentials WHERE user_id = $1 AND revoked_at IS NULL",
        [recovery.owner.id],
      )
    ).rows;
    expect(Number(count)).toBe(0);
    const replacement = agentKey();
    expect((await redeem(recovery.invite.token, replacement)).status).toBe(201);
    expect((await me(await deviceToken(replacement, deviceAudience))).body.userId).toBe(
      recovery.owner.id,
    );
    // The revoked certificate stays revoked.
    const again = await bootstrap();
    const reused = await redeem(again.invite.token, lost.key);
    expect(reused.status).toBe(409);
    expect((await reused.json()).type).toBe(problemTypes.credentialConflict);
  });

  test("redemption and bootstrap leave an audit trail without secrets", async () => {
    const { rows } = await database.client.query(
      "SELECT action, summary, changes::text AS changes FROM audit_logs WHERE entity_type IN ('PairingInvite', 'DeviceCredential', 'Member') ORDER BY occurred_at",
    );
    const actions = new Set(rows.map((row) => row.action));
    for (const action of [
      "memberRegistered",
      "pairingInviteCreated",
      "pairingInviteRevoked",
      "pairingInviteRedeemed",
      "deviceCredentialAdded",
      "deviceCredentialRevoked",
    ]) {
      expect(actions.has(action)).toBe(true);
    }
    expect(rows.some((row) => `${row.summary}${row.changes}`.includes(pairingInvitePrefix))).toBe(
      false,
    );
    const { rows: hosted } = await database.client.query(
      `SELECT DISTINCT "actorType" AS actor_type, source FROM audit_logs WHERE actor_display_name = 'Owner bootstrap'`,
    );
    expect(hosted).toEqual([{ actor_type: "integration", source: "cli" }]);
  });

  test("no invite reached a log line", () => {
    // The capture works: the API's own lines, such as its listening line, were seen.
    expect(logged.some((line) => line.includes("Listening on"))).toBe(true);
    expect(logged.some((line) => line.includes(pairingInvitePrefix))).toBe(false);
  });
});
