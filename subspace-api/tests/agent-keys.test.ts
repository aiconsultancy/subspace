import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { Effect } from "effect";
import { OpenApi } from "effect/http-api";
import { SignJWT } from "jose";
import { agentTokenType, parseCertificate, purgeAgentTokenIds } from "../src/agent-keys";
import { documentedApi } from "../src/api";
import { agentTokenAudience, config } from "../src/config";
import { databaseLayer } from "../src/db";
import { agentKey, agentToken, certificateDer, keyPair, pem, type AgentKey } from "./certificates";
import { entraTenant, isolatedDatabase, startApi, testDatabase } from "./entra-stack";

const audience = "https://api.example.com";

describe("certificates", () => {
  test("an Ed25519 or P-256 certificate is read, and its fingerprint is the SHA-256 of its DER", async () => {
    for (const kind of ["ed25519", "p256"] as const) {
      const key = agentKey(kind);
      const check = await parseCertificate(key.certificate);
      expect(check.ok).toBe(true);
      if (!check.ok) continue;
      expect(check.certificate.fingerprint).toBe(key.fingerprint);
      expect(check.certificate.fingerprint).toMatch(/^[0-9a-f]{64}$/u);
      expect(check.certificate.algorithm).toBe(kind === "ed25519" ? "EdDSA" : "ES256");
      expect(check.certificate.pem).toBe(key.certificate);
      // Windows line endings, one long line, and surrounding whitespace are the same certificate.
      for (const variant of [
        key.certificate.replaceAll("\n", "\r\n"),
        `-----BEGIN CERTIFICATE-----\n${key.der.toString("base64")}\n-----END CERTIFICATE-----`,
        `\n  ${key.certificate}\n`,
      ]) {
        const again = await parseCertificate(variant);
        expect(again.ok && again.certificate.fingerprint).toBe(key.fingerprint);
      }
    }
  });

  test("RSA, other curves and anything malformed are refused", async () => {
    for (const kind of ["rsa", "p384"] as const)
      expect(await parseCertificate(agentKey(kind).certificate)).toEqual({
        ok: false,
        reason: "key",
      });
    const key = agentKey("ed25519");
    const body = key.der.toString("base64");
    const block = (base64: string, label = "CERTIFICATE") =>
      `-----BEGIN ${label}-----\n${base64}\n-----END ${label}-----\n`;
    for (const malformed of [
      "",
      "not a certificate",
      key.der.toString("base64"),
      block(body, "PUBLIC KEY"),
      block(Buffer.from("not DER").toString("base64")),
      // Two certificates, or one with bytes after it.
      `${key.certificate}${agentKey("ed25519").certificate}`,
      block(Buffer.concat([key.der, Buffer.from([0, 0])]).toString("base64")),
      // Truncated, or base64 that is not canonical.
      block(key.der.subarray(0, key.der.length - 1).toString("base64")),
      block(`${body.slice(0, -2)}${body.at(-2) === "A" ? "B" : "A"}=`),
      block(body.replace(/(.{10})/u, "$1 ")),
      `text before\n${key.certificate}`,
      block("A".repeat(20_000)),
    ])
      expect([malformed.slice(0, 40), (await parseCertificate(malformed)).ok]).toEqual([
        malformed.slice(0, 40),
        false,
      ]);
  });

  test("validity dates are not enforced: only the key and the fingerprint matter", async () => {
    const keys = keyPair("ed25519");
    const expired = certificateDer("ed25519", keys, {
      notBefore: new Date("2020-01-01T00:00:00Z"),
      notAfter: new Date("2021-01-01T00:00:00Z"),
    });
    expect((await parseCertificate(pem(expired))).ok).toBe(true);
    const notYet = certificateDer("ed25519", keys, {
      notBefore: new Date("2040-01-01T00:00:00Z"),
      notAfter: new Date("2041-01-01T00:00:00Z"),
    });
    expect((await parseCertificate(pem(notYet))).ok).toBe(true);
  });
});

describe("AGENT_TOKEN_AUDIENCE", () => {
  test("it is the API's public origin, or unset", () => {
    expect(agentTokenAudience({})).toBeNull();
    expect(agentTokenAudience({ AGENT_TOKEN_AUDIENCE: " " })).toBeNull();
    for (const origin of [
      "https://api.example.com",
      "https://api.example.com:8443",
      "http://127.0.0.1:3000",
      "http://[::1]:3000",
    ])
      expect(agentTokenAudience({ AGENT_TOKEN_AUDIENCE: origin })).toBe(origin);
    for (const value of [
      "https://api.example.com/",
      "https://api.example.com/api",
      "https://API.example.com",
      "https://api.example.com:443",
      "https://user@api.example.com",
      "https://api.example.com?x=1",
      "api.example.com",
      "ftp://api.example.com",
    ])
      expect(() => agentTokenAudience({ AGENT_TOKEN_AUDIENCE: value })).toThrow(
        "AGENT_TOKEN_AUDIENCE must be the API's public origin",
      );
    // Any identity provider for people, in production.
    expect(
      config({
        DATABASE_URL: "postgresql://subspace@127.0.0.1/subspace",
        NODE_ENV: "production",
        WORKOS_CLIENT_ID: "client_example",
        AGENT_TOKEN_AUDIENCE: audience,
      }).agentTokenAudience,
    ).toBe(audience);
  });
});

test("the credential routes are documented", () => {
  const document = OpenApi.fromApi(documentedApi) as {
    paths: Record<string, Record<string, unknown>>;
  };
  expect(Object.keys(document.paths["/api/v1/agents/{agentId}/credentials"]!).sort()).toEqual([
    "get",
    "post",
  ]);
  expect(
    Object.keys(document.paths["/api/v1/agents/{agentId}/credentials/{credentialId}"]!),
  ).toEqual(["delete"]);
});

describe.skipIf(!testDatabase)("agent tokens on the main listener", () => {
  let database: Awaited<ReturnType<typeof isolatedDatabase>>;
  let tenant: Awaited<ReturnType<typeof entraTenant>>;
  let api: Awaited<ReturnType<typeof startApi>>;
  let unconfigured: Awaited<ReturnType<typeof startApi>>;
  let admin: string;
  let member: string;
  const adminRole = "Subspace.Admin";

  beforeAll(async () => {
    database = await isolatedDatabase("agent_keys");
    tenant = await entraTenant();
    const env = { ...tenant.env, NODE_ENV: "production", ENTRA_ADMIN_ROLE: adminRole };
    api = await startApi(
      database.url,
      {
        ...env,
        AGENT_TOKEN_AUDIENCE: audience,
        IRC_CREDENTIAL_SECRET: "an example credential secret of at least 32 bytes",
        IRC_AUTH_SERVERS: "irc.example.com:6697",
      },
      tenant.fetchRemote,
    );
    unconfigured = await startApi(database.url, env, tenant.fetchRemote);
    admin = await tenant.sign(crypto.randomUUID(), {
      given_name: "Kerensa",
      family_name: "Doble",
      roles: [adminRole],
    });
    member = await tenant.sign(crypto.randomUUID(), { given_name: "Morwen" });
    expect((await api.call("/api/v1/me", member)).status).toBe(200);
    const fleet = await api.call("/api/v1/fleets", admin, "POST", {
      name: "Example fleet",
      slug: "keys",
      ircHost: "irc.example.com",
      ircPort: 6697,
      ircTls: true,
    });
    expect(fleet.status).toBe(201);
  });
  afterAll(async () => {
    await api?.close();
    await unconfigured?.close();
    await database?.drop();
  });

  let agents = 0;
  /** A rear admiral registered by the administrator, with an IRC nick. */
  const registerAgent = async () => {
    const name = `Keyed-${++agents}`;
    const response = await api.call("/api/v1/agents", admin, "POST", {
      name,
      rank: "rearAdmiral",
      fleet: "keys",
      defaultModel: "example-model",
      serviceTokenClientId: crypto.randomUUID(),
      ircNick: name,
    });
    expect(response.status).toBe(201);
    return (await response.json()) as { userId: string; name: string; version: number };
  };
  const addKey = async (agentId: string, key: AgentKey, scopes = ["api"], label?: string) => {
    const response = await api.call(`/api/v1/agents/${agentId}/credentials`, admin, "POST", {
      certificate: key.certificate,
      scopes,
      ...(label ? { label } : {}),
    });
    expect(response.status).toBe(201);
    return (await response.json()) as { id: string; fingerprint: string };
  };
  const keyedAgent = async (kind: "ed25519" | "p256" = "ed25519", scopes = ["api"]) => {
    const agent = await registerAgent();
    const key = agentKey(kind);
    const credential = await addKey(agent.userId, key, scopes);
    return { agent, key, credential };
  };
  const me = async (token: string, server = api) => {
    const response = await server.call("/api/v1/me", token);
    return { status: response.status, response, body: await response.json() };
  };
  const mcp = (token: string, method: string, params?: unknown) =>
    api.call("/mcp", token, "POST", { jsonrpc: "2.0", id: 1, method, params });

  test("an agent with a freshly generated Ed25519 or P-256 key is that agent in production", async () => {
    for (const kind of ["ed25519", "p256"] as const) {
      const { agent, key, credential } = await keyedAgent(kind);
      expect(credential.fingerprint).toBe(key.fingerprint);
      // Minted as a client would: header alg, typ and kid; claims iss, sub, aud, iat, exp, jti.
      const iat = Math.floor(Date.now() / 1000);
      const token = await new SignJWT({
        iss: "subspace-agent",
        sub: key.fingerprint,
        aud: audience,
        iat,
        exp: iat + 300,
        jti: crypto.randomUUID(),
      })
        .setProtectedHeader({
          alg: kind === "ed25519" ? "EdDSA" : "ES256",
          typ: "subspace-agent+jwt",
          kid: key.fingerprint,
        })
        .sign(key.privateKey);
      const read = await me(token);
      expect(read.status).toBe(200);
      expect(read.response.headers.get("cache-control")).toBe("private, no-store");
      expect(read.body).toMatchObject({
        userId: agent.userId,
        kind: "agent",
        agentName: agent.name,
        agentRank: "rearAdmiral",
        fleet: "keys",
        isAdmin: false,
      });
      const tools = await mcp(await agentToken(key, audience), "tools/list");
      expect(tools.status).toBe(200);
      expect(((await tools.json()).result.tools as unknown[]).length).toBeGreaterThan(0);
    }
  });

  test("a token is accepted once: a replayed jti is refused, on this or any instance", async () => {
    const { key } = await keyedAgent();
    const token = await agentToken(key, audience);
    expect((await me(token)).status).toBe(200);
    expect((await me(token)).status).toBe(401);
    const second = await startApi(
      database.url,
      { ...tenant.env, NODE_ENV: "production", AGENT_TOKEN_AUDIENCE: audience },
      tenant.fetchRemote,
    );
    try {
      expect((await me(token, second)).status).toBe(401);
      const fresh = await agentToken(key, audience);
      expect((await me(fresh, second)).status).toBe(200);
      expect((await me(fresh)).status).toBe(401);
    } finally {
      await second.close();
    }
    // The same jti from another credential is another token.
    const other = await keyedAgent();
    const jti = crypto.randomUUID();
    expect((await me(await agentToken(key, audience, { claims: { jti } }))).status).toBe(200);
    expect((await me(await agentToken(other.key, audience, { claims: { jti } }))).status).toBe(200);
  });

  test("each refusal: key, times, lifetime, audience, typ, alg, kid, subject, issuer", async () => {
    const { key } = await keyedAgent("ed25519");
    const p256 = await keyedAgent("p256");
    const stranger = agentKey("ed25519");
    const now = Math.floor(Date.now() / 1000);
    const unsigned = (header: object, claims: object) =>
      `${Buffer.from(JSON.stringify(header)).toString("base64url")}.${Buffer.from(
        JSON.stringify(claims),
      ).toString("base64url")}.`;
    const refusals: Record<string, string | Promise<string>> = {
      "wrong key": agentToken(key, audience, { signWith: stranger.privateKey }),
      expired: agentToken(key, audience, { issuedAt: now - 400 }),
      "too long a lifetime": agentToken(key, audience, { lifetime: 301 }),
      "no lifetime": agentToken(key, audience, { lifetime: 0 }),
      "issued in the future": agentToken(key, audience, { issuedAt: now + 120, lifetime: 60 }),
      "wrong audience": agentToken(key, "https://other.example.com"),
      "audience with a path": agentToken(key, `${audience}/`),
      "audience in an array": agentToken(key, audience, { claims: { aud: [audience] } }),
      "typ JWT": agentToken(key, audience, { header: { typ: "JWT" } }),
      "typ in another case": agentToken(key, audience, { header: { typ: "Subspace-Agent+JWT" } }),
      "no typ": agentToken(key, audience, { header: { typ: undefined } }),
      "alg HS256 keyed by the certificate": agentToken(key, audience, {
        header: { alg: "HS256" },
        signWith: new TextEncoder().encode(key.certificate),
      }),
      "alg none": unsigned(
        { alg: "none", typ: agentTokenType, kid: key.fingerprint },
        {
          iss: "subspace-agent",
          sub: key.fingerprint,
          aud: audience,
          iat: now,
          exp: now + 60,
          jti: crypto.randomUUID(),
        },
      ),
      "alg ES256 for an Ed25519 key": agentToken(p256.key, audience, {
        header: { kid: key.fingerprint },
        claims: { sub: key.fingerprint },
      }),
      "alg EdDSA for a P-256 key": agentToken(key, audience, {
        header: { kid: p256.key.fingerprint },
        claims: { sub: p256.key.fingerprint },
      }),
      "unknown fingerprint": agentToken(stranger, audience),
      "kid not a fingerprint": agentToken(key, audience, { header: { kid: "agent" } }),
      "subject not the fingerprint": agentToken(key, audience, {
        claims: { sub: p256.key.fingerprint },
      }),
      "no jti": agentToken(key, audience, { claims: { jti: undefined } }),
      "another issuer": agentToken(key, audience, { claims: { iss: "subspace-dev" } }),
    };
    for (const [reason, token] of Object.entries(refusals)) {
      const response = await me(await token);
      expect([reason, response.status]).toEqual([reason, 401]);
    }
    // The fingerprint compares as hex in either case; the key itself is unchanged.
    const upper = await agentToken(key, audience, {
      header: { kid: key.fingerprint.toUpperCase() },
    });
    expect((await me(upper)).status).toBe(200);
    expect((await me(await agentToken(key, audience))).status).toBe(200);
  });

  test("agent tokens are refused when AGENT_TOKEN_AUDIENCE is not set", async () => {
    const { key } = await keyedAgent();
    const response = await me(await agentToken(key, audience), unconfigured);
    expect(response.status).toBe(401);
  });

  test("a revoked credential is refused and the agent's other credential still works", async () => {
    const { agent, key, credential } = await keyedAgent();
    const spare = agentKey("p256");
    await addKey(agent.userId, spare, ["api", "irc"], "spare");
    expect((await me(await agentToken(spare, audience))).status).toBe(200);
    const revoked = await api.call(
      `/api/v1/agents/${agent.userId}/credentials/${credential.id}`,
      admin,
      "DELETE",
    );
    expect(revoked.status).toBe(200);
    expect((await revoked.json()).revokedAt).toEqual(expect.any(String));
    expect((await me(await agentToken(key, audience))).status).toBe(401);
    expect((await me(await agentToken(spare, audience))).status).toBe(200);
    // Revoking again changes nothing; the record is kept.
    const again = await api.call(
      `/api/v1/agents/${agent.userId}/credentials/${credential.id}`,
      admin,
      "DELETE",
    );
    expect(again.status).toBe(200);
    const list = await (await api.call(`/api/v1/agents/${agent.userId}/credentials`, admin)).json();
    expect(list.items.map((item: { fingerprint: string }) => item.fingerprint).sort()).toEqual(
      [key.fingerprint, spare.fingerprint].sort(),
    );
    expect(list.items.find((item: { label: string }) => item.label === "spare")).toMatchObject({
      scopes: ["api", "irc"],
      algorithm: "ES256",
      revokedAt: null,
      lastUsedAt: expect.any(String),
      createdByUserId: expect.any(String),
    });
    // A revoked key cannot be registered again, here or to another agent.
    const readd = await api.call(`/api/v1/agents/${agent.userId}/credentials`, admin, "POST", {
      certificate: key.certificate,
      scopes: ["api"],
    });
    expect(readd.status).toBe(409);
  });

  test("moving an agent to another fleet revokes its keys; other edits keep them", async () => {
    const { agent, key, credential } = await keyedAgent();
    const spare = agentKey("p256");
    await addKey(agent.userId, spare, ["api", "irc"]);
    const current = await (await api.call(`/api/v1/agents/${agent.userId}`, admin)).json();
    const edit = (fields: Record<string, unknown>, version: number) =>
      api.call(`/api/v1/agents/${agent.userId}`, admin, "PUT", {
        name: current.name,
        rank: current.rank,
        fleet: current.fleet,
        defaultModel: current.defaultModel,
        serviceTokenClientId: current.serviceTokenClientId,
        ircNick: current.ircNick,
        ...fields,
        version,
      });
    // An edit within the fleet keeps every key.
    const kept = await edit({ defaultModel: "another-model" }, current.version);
    expect(kept.status).toBe(200);
    expect((await me(await agentToken(key, audience))).status).toBe(200);
    const fleet = await api.call("/api/v1/fleets", admin, "POST", {
      name: "Other fleet",
      slug: `other-${agents}`,
      ircHost: "irc.example.com",
      ircPort: 6697,
      ircTls: true,
    });
    expect(fleet.status).toBe(201);
    const other = await fleet.json();
    const moved = await edit({ fleet: other.slug }, (await kept.json()).version);
    expect(moved.status).toBe(200);
    expect((await moved.json()).fleetId).toBe(other.id);
    // The previous fleet's gateway can no longer act as the agent in its new fleet.
    expect((await me(await agentToken(key, audience))).status).toBe(401);
    expect((await me(await agentToken(spare, audience))).status).toBe(401);
    const list = await (await api.call(`/api/v1/agents/${agent.userId}/credentials`, admin)).json();
    const adminId = (await (await api.call("/api/v1/me", admin)).json()).userId;
    for (const item of list.items)
      expect(item).toMatchObject({ revokedAt: expect.any(String), revokedByUserId: adminId });
    const audit = await database.client.query(
      "SELECT changes FROM audit_logs WHERE action = 'agentCredentialRevoked' AND entity_id = $1",
      [credential.id],
    );
    expect(audit.rows.map((row) => row.changes)).toEqual([
      expect.objectContaining({
        agentId: agent.userId,
        reason: "fleetChanged",
        toFleetId: other.id,
      }),
    ]);
    // A fresh key, registered in the new fleet, works.
    const fresh = agentKey();
    await addKey(agent.userId, fresh);
    expect((await me(await agentToken(fresh, audience))).body.fleetId).toBe(other.id);
  });

  test("a deactivated agent is refused with 403, and a credential without api is refused", async () => {
    const { agent, key } = await keyedAgent();
    const deactivate = await api.call(
      `/api/v1/agents/${agent.userId}/deactivate`,
      admin,
      "POST",
      {},
    );
    expect(deactivate.status).toBe(200);
    const refused = await me(await agentToken(key, audience));
    expect(refused.status).toBe(403);
    expect(refused.body.detail).toBe(
      `Agent '${agent.name}' is deactivated. Ask a human to reactivate it.`,
    );
    await api.call(`/api/v1/agents/${agent.userId}/reactivate`, admin, "POST", {});
    expect((await me(await agentToken(key, audience))).status).toBe(200);

    const ircOnly = await keyedAgent("ed25519", ["irc"]);
    expect((await me(await agentToken(ircOnly.key, audience))).status).toBe(401);
  });

  test("an agent token never reaches an administrator route or MCP tool", async () => {
    const { agent, key } = await keyedAgent();
    const token = () => agentToken(key, audience);
    const fleet = await api.call("/api/v1/fleets", await token(), "POST", {
      name: "Rogue fleet",
      slug: "rogue",
      ircHost: "irc.example.com",
      ircPort: 6697,
      ircTls: true,
    });
    expect(fleet.status).toBe(403);
    const register = await api.call("/api/v1/agents", await token(), "POST", {
      name: "Rogue",
      rank: "captain",
      fleet: "keys",
      ship: "Example",
      defaultModel: "example-model",
      serviceTokenClientId: crypto.randomUUID(),
    });
    expect(register.status).toBe(403);
    const credentials = await api.call(`/api/v1/agents/${agent.userId}/credentials`, await token());
    expect(credentials.status).toBe(403);
    const add = await api.call(
      `/api/v1/agents/${agent.userId}/credentials`,
      await token(),
      "POST",
      {
        certificate: agentKey().certificate,
        scopes: ["api"],
      },
    );
    expect(add.status).toBe(403);
    const tool = await mcp(await token(), "tools/call", {
      name: "subspace_create_fleet",
      arguments: {
        fleet: {
          name: "Rogue fleet",
          slug: "rogue",
          ircHost: "irc.example.com",
          ircPort: 6697,
          ircTls: true,
        },
      },
    });
    expect(tool.status).toBe(200);
    const result = (await tool.json()).result;
    expect(result.isError).toBe(true);
    expect(result.structuredContent.status).toBe(403);
  });

  test("an agent gets an IRC credential only with the irc scope", async () => {
    const server = { host: "irc.example.com", port: 6697 };
    const apiOnly = await keyedAgent("ed25519", ["api"]);
    const refused = await api.call(
      "/api/v1/me/irc-credential",
      await agentToken(apiOnly.key, audience),
      "POST",
      server,
    );
    expect(refused.status).toBe(403);
    const both = await keyedAgent("p256", ["api", "irc"]);
    const issued = await api.call(
      "/api/v1/me/irc-credential",
      await agentToken(both.key, audience),
      "POST",
      server,
    );
    expect(issued.status).toBe(200);
    expect((await issued.json()).account).toBe(both.agent.name);
  });

  test("administrators manage credentials; certificates are read strictly; others are refused", async () => {
    const agent = await registerAgent();
    const path = `/api/v1/agents/${agent.userId}/credentials`;
    const add = (body: unknown, token = admin) => api.call(path, token, "POST", body);
    const key = agentKey();
    expect((await add({ certificate: key.certificate, scopes: ["api"] }, member)).status).toBe(403);
    expect((await api.call(path, member)).status).toBe(403);
    for (const body of [
      { certificate: agentKey("rsa").certificate, scopes: ["api"] },
      { certificate: agentKey("p384").certificate, scopes: ["api"] },
      { certificate: "not a certificate", scopes: ["api"] },
      { certificate: key.certificate, scopes: [] },
      { certificate: key.certificate, scopes: ["api", "api"] },
      { certificate: key.certificate, scopes: ["admin"] },
      { certificate: key.certificate },
      { certificate: key.certificate, scopes: ["api"], agentId: crypto.randomUUID() },
    ]) {
      const response = await add(body);
      expect([JSON.stringify(body).slice(0, 60), response.status]).toEqual([
        JSON.stringify(body).slice(0, 60),
        400,
      ]);
    }
    const rsa = await (
      await add({ certificate: agentKey("rsa").certificate, scopes: ["api"] })
    ).json();
    expect(rsa.errors).toEqual({
      certificate: ["Use a certificate with an Ed25519 or ECDSA P-256 key."],
    });
    const created = await add({
      certificate: key.certificate,
      scopes: ["irc", "api"],
      label: "vm",
    });
    expect(created.status).toBe(201);
    const credential = await created.json();
    expect(credential).toMatchObject({
      agentId: agent.userId,
      fingerprint: key.fingerprint,
      algorithm: "EdDSA",
      scopes: ["api", "irc"],
      label: "vm",
      certificate: key.certificate,
      lastUsedAt: null,
      revokedAt: null,
    });
    // The same certificate again, for this agent or another, is refused.
    expect((await add({ certificate: key.certificate, scopes: ["api"] })).status).toBe(409);
    const other = await registerAgent();
    expect(
      (
        await api.call(`/api/v1/agents/${other.userId}/credentials`, admin, "POST", {
          certificate: key.certificate,
          scopes: ["api"],
        })
      ).status,
    ).toBe(409);
    // A person is not an agent, and an unknown ID or credential is not found.
    const person = await (await api.call("/api/v1/me", member)).json();
    expect((await api.call(`/api/v1/agents/${person.userId}/credentials`, admin)).status).toBe(404);
    expect(
      (await api.call(`/api/v1/agents/${crypto.randomUUID()}/credentials`, admin)).status,
    ).toBe(404);
    expect(
      (
        await api.call(
          `${path.replace(agent.userId, other.userId)}/${credential.id}`,
          admin,
          "DELETE",
        )
      ).status,
    ).toBe(404);

    const audit = await database.client.query(
      "SELECT action, entity_id, changes FROM audit_logs WHERE entity_id = $1 ORDER BY occurred_at",
      [credential.id],
    );
    expect(audit.rows.map((row) => row.action)).toEqual(["agentCredentialAdded"]);
    expect(JSON.stringify(audit.rows)).not.toContain("BEGIN CERTIFICATE");
    await api.call(`${path}/${credential.id}`, admin, "DELETE");
    const after = await database.client.query(
      "SELECT action FROM audit_logs WHERE entity_id = $1 ORDER BY occurred_at",
      [credential.id],
    );
    expect(after.rows.map((row) => row.action)).toEqual([
      "agentCredentialAdded",
      "agentCredentialRevoked",
    ]);
  });

  test("a certificate active in another organisation cannot be registered here", async () => {
    const key = agentKey();
    const [{ id: org }] = (
      await database.client.query(
        "INSERT INTO organisations (provider, external_id) VALUES ('dev', 'elsewhere-keys') RETURNING id",
      )
    ).rows;
    const [{ id: owner }] = (
      await database.client.query(
        "INSERT INTO users (organisation_id, subject, display_name, role) VALUES ($1, 'owner@example.com', 'Owner', 'admin') RETURNING id",
        [org],
      )
    ).rows;
    const [{ id: elsewhere }] = (
      await database.client.query(
        `INSERT INTO users (organisation_id, subject, display_name, agent_rank, agent_fleet, agent_model, registered_by_id)
         VALUES ($1, 'agent@example.com', 'Elsewhere', 'crew', 'alpha', 'example-model', $2) RETURNING id`,
        [org, owner],
      )
    ).rows;
    const parsed = await parseCertificate(key.certificate);
    if (!parsed.ok) throw new Error("The test certificate did not parse.");
    await database.client.query(
      `INSERT INTO agent_credentials (organisation_id, user_id, fingerprint, algorithm, certificate, public_key, scopes)
       VALUES ($1, $2, $3, 'EdDSA', $4, $5, '{api}')`,
      [org, elsewhere, key.fingerprint, parsed.certificate.pem, parsed.certificate.publicKey],
    );
    const agent = await registerAgent();
    const response = await api.call(`/api/v1/agents/${agent.userId}/credentials`, admin, "POST", {
      certificate: key.certificate,
      scopes: ["api"],
    });
    expect(response.status).toBe(409);
    // Its token is never this organisation's agent's; and an organisation whose identity provider
    // this deployment does not accept is not served at all.
    expect((await me(await agentToken(key, audience))).status).toBe(401);
    await database.client.query("UPDATE organisations SET provider = 'entra' WHERE id = $1", [org]);
    const read = await me(await agentToken(key, audience));
    expect(read.status).toBe(200);
    expect(read.body.userId).toBe(elsewhere);
  });

  test("accepted tokens are recorded until shortly after they expire, then purged", async () => {
    const { credential, key } = await keyedAgent();
    const jti = crypto.randomUUID();
    expect((await me(await agentToken(key, audience, { claims: { jti } }))).status).toBe(200);
    const recorded = await database.client.query(
      "SELECT expires_at FROM agent_token_ids WHERE credential_id = $1 AND jti = $2",
      [credential.id, jti],
    );
    expect(recorded.rows).toHaveLength(1);
    const expires = (recorded.rows[0].expires_at as Date).valueOf();
    expect(expires).toBeGreaterThan(Date.now() + 290_000);
    expect(expires).toBeLessThanOrEqual(Date.now() + 306_000);
    await database.client.query(
      "INSERT INTO agent_token_ids (credential_id, jti, expires_at) VALUES ($1, 'old', now() - interval '6 minutes'), ($1, 'recent', now() - interval '1 minute')",
      [credential.id],
    );
    await Effect.runPromise(
      purgeAgentTokenIds.pipe(Effect.provide(databaseLayer(database.url, 2))),
    );
    const left = await database.client.query(
      "SELECT jti FROM agent_token_ids WHERE credential_id = $1 ORDER BY jti",
      [credential.id],
    );
    expect(left.rows.map((row) => row.jti)).toEqual([jti, "recent"].sort());
  });
});
