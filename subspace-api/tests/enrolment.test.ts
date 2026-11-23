import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { OpenApi } from "effect/http-api";
import { documentedApi } from "../src/api";
import { mintEnrolmentToken, readEnrolmentToken } from "../src/enrolment";
import { agentKey, agentToken, type AgentKey } from "./certificates";
import { entraTenant, isolatedDatabase, startApi, testDatabase } from "./entra-stack";

const audience = "https://api.example.com";

describe("enrolment tokens", () => {
  test("a token is senr1. and 64 characters that name its row, stored only as a hash", () => {
    const id = crypto.randomUUID();
    const { token, hash } = mintEnrolmentToken(id);
    expect(token).toMatch(/^senr1\.[A-Za-z0-9_-]{64}$/u);
    expect(hash).toMatch(/^[0-9a-f]{64}$/u);
    expect(hash).not.toContain(token.slice(6));
    const read = readEnrolmentToken(token);
    expect(read?.id).toBe(id);
    expect(read?.hash.toString("hex")).toBe(hash);
    expect(mintEnrolmentToken(id).token).not.toBe(token);
    for (const malformed of [
      "",
      token.replace("senr1.", "senr2."),
      token.slice(0, -1),
      `${token}A`,
      `${token.slice(0, -1)}=`,
      `${token.slice(0, -1)}+`,
    ])
      expect(readEnrolmentToken(malformed)).toBeUndefined();
  });

  test("enrolment is documented with its own security scheme; token management with a member's", () => {
    const document = OpenApi.fromApi(documentedApi) as {
      paths: Record<string, Record<string, { security?: unknown }>>;
    };
    expect(document.paths["/api/v1/agents/enrol"]!.post!.security).toEqual([
      { enrolmentToken: [] },
    ]);
    const tokens = document.paths["/api/v1/fleets/{reference}/enrolment-tokens"]!;
    expect(Object.keys(tokens).sort()).toEqual(["get", "post"]);
    expect(tokens.post!.security).toEqual([{ bearer: [] }]);
    expect(
      Object.keys(document.paths["/api/v1/fleets/{reference}/enrolment-tokens/{tokenId}"]!),
    ).toEqual(["delete"]);
  });
});

describe.skipIf(!testDatabase)("fleet enrolment over HTTP", () => {
  let database: Awaited<ReturnType<typeof isolatedDatabase>>;
  let tenant: Awaited<ReturnType<typeof entraTenant>>;
  let api: Awaited<ReturnType<typeof startApi>>;
  let admin: string;
  let member: string;
  const adminRole = "Subspace.Admin";

  beforeAll(async () => {
    database = await isolatedDatabase("enrolment");
    tenant = await entraTenant();
    api = await startApi(
      database.url,
      {
        ...tenant.env,
        NODE_ENV: "production",
        ENTRA_ADMIN_ROLE: adminRole,
        AGENT_TOKEN_AUDIENCE: audience,
        RESERVED_USERNAMES: "operator",
      },
      tenant.fetchRemote,
    );
    admin = await tenant.sign(crypto.randomUUID(), {
      given_name: "Kerensa",
      family_name: "Doble",
      roles: [adminRole],
    });
    member = await tenant.sign(crypto.randomUUID(), { given_name: "Morwen" });
    expect((await api.call("/api/v1/me", member)).status).toBe(200);
    for (const slug of ["alpha", "beta"]) {
      const fleet = await api.call("/api/v1/fleets", admin, "POST", {
        name: `Fleet ${slug}`,
        slug,
        ircHost: "irc.example.com",
        ircPort: 6697,
        ircTls: true,
      });
      expect(fleet.status).toBe(201);
    }
  });
  afterAll(async () => {
    await api?.close();
    await database?.drop();
  });

  const issue = async (fleet = "alpha", body: unknown = {}) => {
    const response = await api.call(
      `/api/v1/fleets/${fleet}/enrolment-tokens`,
      admin,
      "POST",
      body,
    );
    expect(response.status).toBe(201);
    return (await response.json()) as {
      id: string;
      token: string;
      expiresAt: string;
      maxUses: number;
    };
  };
  const enrol = async (token: string, body: Record<string, unknown>) => {
    const response = await api.call("/api/v1/agents/enrol", token, "POST", body);
    return { status: response.status, response, body: await response.json() };
  };
  const rearAdmiral = (name: string, key: AgentKey, extra: Record<string, unknown> = {}) => ({
    name,
    rank: "rearAdmiral",
    model: "example-model",
    ircNick: name,
    certificate: key.certificate,
    scopes: ["api", "irc"],
    ...extra,
  });
  const captain = (name: string, ship: string, key: AgentKey, extra = {}) =>
    rearAdmiral(name, key, { rank: "captain", ship, ...extra });
  const me = async (key: AgentKey) => {
    const response = await api.call("/api/v1/me", await agentToken(key, audience));
    return { status: response.status, body: await response.json() };
  };
  const audit = async (action: string, entityId?: string) =>
    (
      await database.client.query(
        `SELECT "actorType" AS actor_type, actor_id, actor_display_name, entity_id, changes FROM audit_logs
         WHERE action = $1 AND ($2::text IS NULL OR entity_id = $2) ORDER BY occurred_at`,
        [action, entityId ?? null],
      )
    ).rows;

  test("a fleet enrols a rear admiral and two captains with one token and no administrator login", async () => {
    const before = Date.now();
    const issued = await issue();
    expect(issued.token).toMatch(/^senr1\./u);
    expect(issued.maxUses).toBe(16);
    expect(Date.parse(issued.expiresAt)).toBeGreaterThanOrEqual(before + 3_599_000);
    expect(Date.parse(issued.expiresAt)).toBeLessThanOrEqual(Date.now() + 3_600_000);
    const keys = { admiral: agentKey("ed25519"), kestrel: agentKey("p256"), merlin: agentKey() };
    const results = [
      await enrol(issued.token, rearAdmiral("Halyard", keys.admiral)),
      await enrol(issued.token, captain("Kestrel", "Swift", keys.kestrel)),
      await enrol(issued.token, captain("Merlin", "Tern", keys.merlin, { scopes: ["api"] })),
    ];
    for (const result of results) {
      expect(result.status).toBe(201);
      expect(result.response.headers.get("cache-control")).toBe("no-store");
      expect(Object.keys(result.body).sort()).toEqual(["agentId", "credentialId", "fingerprint"]);
    }
    expect(results.map((result) => result.body.fingerprint)).toEqual([
      keys.admiral.fingerprint,
      keys.kestrel.fingerprint,
      keys.merlin.fingerprint,
    ]);
    // Each is that agent on the main listener, in production, and never an administrator.
    const admiral = await me(keys.admiral);
    expect(admiral.body).toMatchObject({
      userId: results[0]!.body.agentId,
      kind: "agent",
      agentRank: "rearAdmiral",
      fleet: "alpha",
      ship: null,
      username: "Halyard",
      isAdmin: false,
    });
    expect((await me(keys.kestrel)).body).toMatchObject({
      userId: results[1]!.body.agentId,
      agentRank: "captain",
      ship: "Swift",
    });
    expect((await me(keys.merlin)).body).toMatchObject({ agentName: "Merlin", ship: "Tern" });
    const fleet = await (await api.call("/api/v1/fleets/alpha", admin)).json();
    expect(fleet.rearAdmiralName).toBe("Halyard");
    expect(fleet.captainNames).toEqual(["Kestrel", "Merlin"]);
    // An enrolled agent has no provider account: its subject is reserved and its own.
    const agent = await (
      await api.call(`/api/v1/agents/${results[0]!.body.agentId}`, admin)
    ).json();
    expect(agent.serviceTokenClientId).toBe(`subspace-agent:${results[0]!.body.agentId}`);
    expect(agent.registeredByUserId).toBe(
      (await (await api.call("/api/v1/me", admin)).json()).userId,
    );
    const credentials = await (
      await api.call(`/api/v1/agents/${results[2]!.body.agentId}/credentials`, admin)
    ).json();
    expect(credentials.items).toEqual([
      expect.objectContaining({
        id: results[2]!.body.credentialId,
        scopes: ["api"],
        enrolmentTokenId: issued.id,
        createdByUserId: null,
      }),
    ]);
    const listed = await (await api.call("/api/v1/fleets/alpha/enrolment-tokens", admin)).json();
    expect(listed.items.find((item: { id: string }) => item.id === issued.id)).toMatchObject({
      uses: 3,
      maxUses: 16,
      revokedAt: null,
    });
    expect(JSON.stringify(listed)).not.toContain(issued.token);

    // Audited with the token's ID, by the token rather than a member, and never its text.
    const used = await audit("enrolmentTokenUsed", issued.id);
    expect(used.map((row) => row.changes.uses)).toEqual([1, 2, 3]);
    expect(used[0]).toMatchObject({
      actor_type: "integration",
      actor_id: null,
      actor_display_name: "Enrolment token",
    });
    const enrolled = await audit("agentEnrolled", results[1]!.body.agentId);
    expect(enrolled[0]!.changes).toMatchObject({ enrolmentTokenId: issued.id, rank: "captain" });
    expect(await audit("agentCredentialAdded", results[1]!.body.credentialId)).toHaveLength(1);
    expect(await audit("enrolmentTokenCreated", issued.id)).toHaveLength(1);
    const all = await database.client.query("SELECT changes, summary FROM audit_logs");
    expect(JSON.stringify(all.rows)).not.toContain(issued.token.slice(6));
    const stored = await database.client.query(
      "SELECT token_hash FROM enrolment_tokens WHERE id = $1",
      [issued.id],
    );
    expect(stored.rows[0].token_hash).not.toContain(issued.token.slice(6));
  });

  test("the same certificate twice answers 200 with the same IDs and uses nothing", async () => {
    const issued = await issue("alpha", { maxUses: 2 });
    const key = agentKey();
    const first = await enrol(issued.token, rearAdmiral("Petrel", key, { ircNick: null }));
    expect(first.status).toBe(201);
    const again = await enrol(issued.token, rearAdmiral("Petrel", key, { ircNick: null }));
    expect(again.status).toBe(200);
    expect(again.body).toEqual(first.body);
    // Another key for the same agent adds a credential, and uses the token again.
    const second = agentKey("p256");
    const added = await enrol(issued.token, rearAdmiral("petrel", second, { ircNick: null }));
    expect(added.status).toBe(201);
    expect(added.body.agentId).toBe(first.body.agentId);
    expect(added.body.credentialId).not.toBe(first.body.credentialId);
    expect((await me(key)).body.userId).toBe(first.body.agentId);
    expect((await me(second)).body.userId).toBe(first.body.agentId);
    // The token is used up for anything new, but a repeat still answers.
    const third = await enrol(issued.token, captain("Fulmar", "Skerry", agentKey()));
    expect(third.status).toBe(401);
    expect(third.body.detail).toBe("This enrolment token has been used up.");
    expect((await enrol(issued.token, rearAdmiral("Petrel", key, { ircNick: null }))).status).toBe(
      200,
    );
    // The same certificate with other scopes is not the same registration.
    const scoped = await enrol(
      issued.token,
      rearAdmiral("Petrel", key, { ircNick: null, scopes: ["api"] }),
    );
    expect(scoped.status).toBe(409);
  });

  test("an expired, used-up or revoked token is refused", async () => {
    const expired = await issue("alpha", { expiresInSeconds: 60 });
    await database.client.query(
      "UPDATE enrolment_tokens SET expires_at = now() - interval '1 second' WHERE id = $1",
      [expired.id],
    );
    const late = await enrol(expired.token, captain("Gannet", "Bass", agentKey()));
    expect(late.status).toBe(401);
    expect(late.body.detail).toBe("This enrolment token has expired.");

    const single = await issue("alpha", { maxUses: 1 });
    expect((await enrol(single.token, captain("Shag", "Rock", agentKey()))).status).toBe(201);
    const exhausted = await enrol(single.token, captain("Skua", "Rock", agentKey()));
    expect(exhausted.status).toBe(401);
    expect(exhausted.body.detail).toBe("This enrolment token has been used up.");

    const revocable = await issue();
    const revoked = await api.call(
      `/api/v1/fleets/alpha/enrolment-tokens/${revocable.id}`,
      admin,
      "DELETE",
    );
    expect(revoked.status).toBe(200);
    expect((await revoked.json()).revokedAt).toEqual(expect.any(String));
    const refused = await enrol(revocable.token, captain("Tystie", "Rock", agentKey()));
    expect(refused.status).toBe(401);
    expect(refused.body.detail).toBe("This enrolment token was revoked.");
    expect(await audit("enrolmentTokenRevoked", revocable.id)).toHaveLength(1);
    // Revoking again changes nothing; another fleet's reference does not find it.
    expect(
      (await api.call(`/api/v1/fleets/alpha/enrolment-tokens/${revocable.id}`, admin, "DELETE"))
        .status,
    ).toBe(200);
    expect(
      (await api.call(`/api/v1/fleets/beta/enrolment-tokens/${revocable.id}`, admin, "DELETE"))
        .status,
    ).toBe(404);
    // None of them registered anything.
    const names = await database.client.query(
      "SELECT display_name FROM users WHERE display_name IN ('Gannet', 'Skua', 'Tystie')",
    );
    expect(names.rows).toEqual([]);
  });

  test("concurrent enrolments never use a token more often than it allows", async () => {
    const issued = await issue("alpha", { maxUses: 2 });
    const results = await Promise.all(
      ["Tern", "Noddy", "Skimmer", "Gadfly", "Prion"].map((name) =>
        enrol(issued.token, captain(`Race-${name}`, "Spray", agentKey())),
      ),
    );
    expect(results.map((result) => result.status).sort()).toEqual([201, 201, 401, 401, 401]);
    const stored = await database.client.query("SELECT uses FROM enrolment_tokens WHERE id = $1", [
      issued.id,
    ]);
    expect(stored.rows[0].uses).toBe(2);
    // Two gateways enrolling the same new agent at once create one agent.
    const twin = await issue();
    const [one, two] = await Promise.all([
      enrol(twin.token, captain("Race-Twin", "Spray", agentKey())),
      enrol(twin.token, captain("Race-Twin", "Spray", agentKey())),
    ]);
    expect([one.status, two.status]).toEqual([201, 201]);
    expect(one.body.agentId).toBe(two.body.agentId);
  });

  test("only an enrolment token authorises enrolment, and a wrong one is refused", async () => {
    const issued = await issue();
    const body = captain("Cormorant", "Ledge", agentKey());
    const tampered = `${issued.token.slice(0, -2)}${issued.token.endsWith("AA") ? "BB" : "AA"}`;
    const { token: forged } = mintEnrolmentToken(issued.id);
    for (const token of [
      undefined,
      "",
      tampered,
      forged,
      mintEnrolmentToken(crypto.randomUUID()).token,
      admin,
      member,
      await agentToken(agentKey(), audience),
    ]) {
      const response = await api.call("/api/v1/agents/enrol", token, "POST", body);
      expect(response.status).toBe(401);
    }
    expect((await enrol(issued.token, body)).status).toBe(201);
  });

  test("a name held in another fleet, and any attempt at another fleet, are refused", async () => {
    const alpha = await issue("alpha");
    const beta = await issue("beta");
    const held = await enrol(beta.token, captain("Shearwater", "Gull", agentKey()));
    expect(held.status).toBe(201);
    const taken = await enrol(alpha.token, captain("shearwater", "Gull", agentKey()));
    expect(taken.status).toBe(409);
    expect(taken.body.detail).toBe("An agent named 'Shearwater' belongs to another fleet.");
    // The body cannot name a fleet: the token's fleet is the only one.
    for (const extra of [{ fleet: "beta" }, { fleetId: crypto.randomUUID() }, { role: "admin" }]) {
      const response = await enrol(alpha.token, captain("Auk", "Puffin", agentKey(), extra));
      expect([extra, response.status]).toEqual([extra, 400]);
    }
    // A key already registered to another fleet's agent is not attached here.
    const betaKey = agentKey();
    expect((await enrol(beta.token, captain("Guillemot", "Gull", betaKey))).status).toBe(201);
    const attached = await enrol(alpha.token, captain("Razorbill", "Ledge", betaKey));
    expect(attached.status).toBe(409);
    expect((await me(betaKey)).body.fleet).toBe("beta");
    // The other fleet's agents are unchanged.
    const fleet = await (await api.call("/api/v1/fleets/beta", admin)).json();
    expect(fleet.captainNames).toEqual(["Guillemot", "Shearwater"]);
  });

  test("an existing agent is used only if active and of the same rank; crew are not enrolled", async () => {
    const issued = await issue();
    const registered = await api.call("/api/v1/agents", admin, "POST", {
      name: "Osprey",
      rank: "captain",
      fleet: "alpha",
      ship: "Eyrie",
      defaultModel: "example-model",
      serviceTokenClientId: crypto.randomUUID(),
    });
    expect(registered.status).toBe(201);
    const osprey = await registered.json();
    const rank = await enrol(issued.token, rearAdmiral("Osprey", agentKey(), { ircNick: null }));
    expect(rank.status).toBe(409);
    // An administrator's agent of the fleet can be given a key by enrolment.
    const key = agentKey();
    const added = await enrol(issued.token, captain("Osprey", "Elsewhere", key, { ircNick: null }));
    expect(added.status).toBe(201);
    expect(added.body.agentId).toBe(osprey.userId);
    expect((await me(key)).body).toMatchObject({ userId: osprey.userId, ship: "Eyrie" });
    await api.call(`/api/v1/agents/${osprey.userId}/deactivate`, admin, "POST", {});
    const inactive = await enrol(issued.token, captain("Osprey", "Eyrie", agentKey()));
    expect(inactive.status).toBe(409);
    for (const body of [
      { ...captain("Wren", "Hedge", agentKey()), rank: "crew" },
      captain("Wren", "", agentKey()),
      { ...captain("Wren", "Hedge", agentKey()), ship: undefined },
      rearAdmiral("Wren", agentKey(), { ship: "Hedge" }),
      captain("Wren", "Hedge", agentKey(), { certificate: agentKey("rsa").certificate }),
      captain("Wren", "Hedge", agentKey(), { scopes: [] }),
      captain("Wren!", "Hedge", agentKey()),
    ]) {
      const response = await enrol(issued.token, body);
      expect([JSON.stringify(body).slice(0, 80), response.status]).toEqual([
        JSON.stringify(body).slice(0, 80),
        400,
      ]);
    }
  });

  test("an enrolled agent's IRC nick follows the username rules", async () => {
    const issued = await issue();
    const person = await (await api.call("/api/v1/me", member)).json();
    const taken = await enrol(
      issued.token,
      captain("Dipper", "Brook", agentKey(), {
        ircNick: person.username.toUpperCase(),
      }),
    );
    expect(taken.status).toBe(409);
    const reserved = await enrol(
      issued.token,
      captain("Dipper", "Brook", agentKey(), {
        ircNick: "Operator",
      }),
    );
    expect(reserved.status).toBe(400);
    expect(
      (await enrol(issued.token, captain("Dipper", "Brook", agentKey(), { ircNick: "NickServ" })))
        .status,
    ).toBe(400);
    expect((await enrol(issued.token, captain("Dipper", "Brook", agentKey()))).status).toBe(201);
  });

  test("an enrolled agent is never matched by a provider's token", async () => {
    const issued = await issue();
    const key = agentKey();
    const enrolled = await enrol(issued.token, captain("Heron", "Reed", key));
    expect(enrolled.status).toBe(201);
    const subject = `subspace-agent:${enrolled.body.agentId}`;
    // A token from the trusted tenant whose subject is the enrolled agent's is refused, and
    // first sign-in registers no one under it.
    const forged = await tenant.sign(subject);
    const response = await api.call("/api/v1/me", forged);
    expect(response.status).toBe(403);
    const holders = await database.client.query("SELECT id FROM users WHERE subject = $1", [
      subject,
    ]);
    expect(holders.rows.map((row) => row.id)).toEqual([enrolled.body.agentId]);
    // Nor may an administrator give such a subject to an agent; editing the agent keeps it.
    const register = await api.call("/api/v1/agents", admin, "POST", {
      name: "Bittern",
      rank: "captain",
      fleet: "alpha",
      ship: "Reed",
      defaultModel: "example-model",
      serviceTokenClientId: `subspace-agent:${crypto.randomUUID()}`,
    });
    expect(register.status).toBe(400);
    const agent = await (await api.call(`/api/v1/agents/${enrolled.body.agentId}`, admin)).json();
    const edit = await api.call(`/api/v1/agents/${agent.userId}`, admin, "PUT", {
      name: agent.name,
      rank: agent.rank,
      fleetId: agent.fleetId,
      shipId: agent.shipId,
      defaultModel: "another-model",
      serviceTokenClientId: agent.serviceTokenClientId,
      ircNick: agent.ircNick,
      version: agent.version,
    });
    expect(edit.status).toBe(200);
    expect((await me(key)).status).toBe(200);
  });

  test("tokens are managed by administrators only, within bounds", async () => {
    const path = "/api/v1/fleets/alpha/enrolment-tokens";
    expect((await api.call(path, member, "POST", {})).status).toBe(403);
    expect((await api.call(path, member)).status).toBe(403);
    const issued = await issue();
    const key = agentKey();
    expect((await enrol(issued.token, captain("Plover", "Shore", key))).status).toBe(201);
    const agent = await api.call(path, await agentToken(key, audience), "POST", {});
    expect(agent.status).toBe(403);
    expect((await api.call(`${path}/${issued.id}`, member, "DELETE")).status).toBe(403);
    expect(
      (await api.call("/api/v1/fleets/unknown/enrolment-tokens", admin, "POST", {})).status,
    ).toBe(404);
    for (const body of [
      { expiresInSeconds: 59 },
      { expiresInSeconds: 604_801 },
      { expiresInSeconds: 600.5 },
      { maxUses: 0 },
      { maxUses: 101 },
      { fleetId: crypto.randomUUID() },
    ])
      expect([body, (await api.call(path, admin, "POST", body)).status]).toEqual([body, 400]);
    const custom = await issue("alpha", { expiresInSeconds: 600, maxUses: 3 });
    expect(custom.maxUses).toBe(3);
    expect(Date.parse(custom.expiresAt)).toBeLessThanOrEqual(Date.now() + 600_000);
    // By the fleet's ID as well as its slug.
    const fleet = await (await api.call("/api/v1/fleets/alpha", admin)).json();
    expect((await api.call(`/api/v1/fleets/${fleet.id}/enrolment-tokens`, admin)).status).toBe(200);
  });
});
