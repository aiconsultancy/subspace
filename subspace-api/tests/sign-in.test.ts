import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { jwtVerifiers } from "../src/auth";
import { autoRegister, config } from "../src/config";
import { profileFromClaims } from "../src/registration";
import { entraTenant, isolatedDatabase, startApi, testDatabase } from "./entra-stack";

const databaseUrl = "postgresql://subspace@127.0.0.1/subspace";
const entraEnv = {
  DATABASE_URL: databaseUrl,
  AUTH_PROVIDERS: "entra",
  ENTRA_TENANT_ID: "00000000-0000-4000-8000-000000000001",
  ENTRA_CLIENT_ID: "00000000-0000-4000-8000-000000000002",
  ENTRA_AUDIENCE: "00000000-0000-4000-8000-000000000003",
};

describe("first sign-in configuration", () => {
  test("AUTO_REGISTER defaults to entra when Entra is configured, and is off otherwise", () => {
    expect([...config(entraEnv).autoRegister]).toEqual(["entra"]);
    expect([
      ...config({ ...entraEnv, AUTH_PROVIDERS: "workos,entra", WORKOS_CLIENT_ID: "client_example" })
        .autoRegister,
    ]).toEqual(["entra"]);
    expect([
      ...config({ DATABASE_URL: databaseUrl, WORKOS_CLIENT_ID: "client_example" }).autoRegister,
    ]).toEqual([]);
    expect([
      ...config({
        DATABASE_URL: databaseUrl,
        AUTH_PROVIDERS: "dev",
        ENABLE_DEV_AUTH: "true",
      }).autoRegister,
    ]).toEqual([]);
  });

  test("an empty AUTO_REGISTER or none turns registration off", () => {
    for (const value of ["", " ", "none", " none "])
      expect([...config({ ...entraEnv, AUTO_REGISTER: value }).autoRegister]).toEqual([]);
    expect([...config({ ...entraEnv, AUTO_REGISTER: " entra " }).autoRegister]).toEqual(["entra"]);
  });

  test("AUTO_REGISTER refuses WorkOS, development sign-in, unknown and unconfigured providers", () => {
    const both = {
      ...entraEnv,
      AUTH_PROVIDERS: "workos,entra",
      WORKOS_CLIENT_ID: "client_example",
    };
    expect(() => config({ ...both, AUTO_REGISTER: "workos" })).toThrow(
      "WorkOS registration needs an organisation allow-list",
    );
    expect(() => config({ ...both, AUTO_REGISTER: "entra,workos" })).toThrow("workos");
    expect(() => autoRegister({ AUTO_REGISTER: "dev" }, new Set(["dev"]))).toThrow(
      "AUTO_REGISTER must be entra, none or empty",
    );
    expect(() => config({ ...entraEnv, AUTO_REGISTER: "none,entra" })).toThrow("AUTO_REGISTER");
    expect(() => config({ ...entraEnv, AUTO_REGISTER: "okta" })).toThrow("AUTO_REGISTER");
    expect(() =>
      config({
        DATABASE_URL: databaseUrl,
        WORKOS_CLIENT_ID: "client_example",
        AUTO_REGISTER: "entra",
      }),
    ).toThrow("AUTO_REGISTER must name a provider in AUTH_PROVIDERS");
  });

  test("ENTRA_ADMIN_ROLE is one role value and needs Entra", () => {
    expect(config(entraEnv).entra?.adminRole).toBeNull();
    expect(config({ ...entraEnv, ENTRA_ADMIN_ROLE: " Subspace.Admin " }).entra?.adminRole).toBe(
      "Subspace.Admin",
    );
    expect(() => config({ ...entraEnv, ENTRA_ADMIN_ROLE: "Two Roles" })).toThrow(
      "ENTRA_ADMIN_ROLE",
    );
    expect(() =>
      config({
        DATABASE_URL: databaseUrl,
        WORKOS_CLIENT_ID: "client_example",
        ENTRA_ADMIN_ROLE: "Subspace.Admin",
      }),
    ).toThrow("ENTRA_ADMIN_ROLE needs entra");
  });
});

test("the Entra verifier reads profile and role claims beside the identity", async () => {
  const tenant = await entraTenant();
  const verify = jwtVerifiers(
    config({ ...tenant.env, DATABASE_URL: databaseUrl }),
    null,
    tenant.fetchRemote,
  );
  const oid = crypto.randomUUID();
  const identity = await verify(
    await tenant.sign(oid, {
      name: " Tamsin Quarrie ",
      email: "tamsin@example.com",
      preferred_username: "tamsin.quarrie@example.com",
      given_name: "Tamsin",
      family_name: "Quarrie",
      roles: ["Subspace.Admin", 7, "Reader"],
    }),
  );
  expect(identity).toEqual({
    provider: "entra",
    subject: oid,
    externalOrganisation: tenant.tenant,
    expiresAt: expect.any(Number),
    claims: {
      name: "Tamsin Quarrie",
      email: "tamsin@example.com",
      preferredUsername: "tamsin.quarrie@example.com",
      givenName: "Tamsin",
      familyName: "Quarrie",
      roles: ["Subspace.Admin", "Reader"],
    },
  });
  // Claims of the wrong type or length are ignored; the identity is unchanged.
  const odd = await verify(
    await tenant.sign(oid, { name: 42, email: "x".repeat(300), roles: "Subspace.Admin" }),
  );
  expect(odd?.subject).toBe(oid);
  expect(odd?.claims).toEqual({
    name: undefined,
    email: undefined,
    preferredUsername: undefined,
    givenName: undefined,
    familyName: undefined,
    roles: [],
  });
});

describe("profile claims at registration", () => {
  test("the display name is name, then the given and family names, then the e-mail", () => {
    expect(profileFromClaims({ name: "Tamsin Quarrie", givenName: "Morva" }).displayName).toBe(
      "Tamsin Quarrie",
    );
    expect(profileFromClaims({ givenName: "Tamsin", familyName: "Quarrie" }).displayName).toBe(
      "Tamsin Quarrie",
    );
    expect(profileFromClaims({ familyName: "Quarrie" }).displayName).toBe("Quarrie");
    expect(profileFromClaims({ email: "tamsin@example.com" }).displayName).toBe(
      "tamsin@example.com",
    );
    expect(profileFromClaims({}).displayName).toBe("New member");
  });

  test("the e-mail is email, else preferred_username, only when it looks like an address", () => {
    expect(
      profileFromClaims({ email: "tamsin@example.com", preferredUsername: "al@example.com" }).email,
    ).toBe("tamsin@example.com");
    expect(profileFromClaims({ preferredUsername: "tamsin@example.com" }).email).toBe(
      "tamsin@example.com",
    );
    expect(
      profileFromClaims({ email: "not an address", preferredUsername: "tamsin@example.com" }).email,
    ).toBe("tamsin@example.com");
    expect(profileFromClaims({ preferredUsername: "ADA\\quarrie" }).email).toBeNull();
  });

  test("names become one line of at most 120 characters", () => {
    expect(profileFromClaims({ name: "Tamsin\n\tQuarrie\u0000" }).displayName).toBe(
      "Tamsin Quarrie",
    );
    expect(profileFromClaims({ name: "A".repeat(200) }).displayName).toHaveLength(120);
  });
});

describe.skipIf(!testDatabase)("registration on the first Entra sign-in", () => {
  let database: Awaited<ReturnType<typeof isolatedDatabase>>;
  let tenant: Awaited<ReturnType<typeof entraTenant>>;
  let api: Awaited<ReturnType<typeof startApi>>;
  let admins: Awaited<ReturnType<typeof startApi>>;
  let closed: Awaited<ReturnType<typeof startApi>>;
  const adminRole = "Subspace.Admin";

  beforeAll(async () => {
    database = await isolatedDatabase("sign_in");
    tenant = await entraTenant();
    api = await startApi(database.url, tenant.env, tenant.fetchRemote);
    admins = await startApi(
      database.url,
      { ...tenant.env, ENTRA_ADMIN_ROLE: adminRole },
      tenant.fetchRemote,
    );
    closed = await startApi(
      database.url,
      { ...tenant.env, AUTO_REGISTER: "none" },
      tenant.fetchRemote,
    );
  });
  afterAll(async () => {
    await api?.close();
    await admins?.close();
    await closed?.close();
    await database?.drop();
  });

  const count = async (table: string, where = "true", values: unknown[] = []) =>
    Number(
      (await database.client.query(`SELECT count(*) AS n FROM ${table} WHERE ${where}`, values))
        .rows[0].n,
    );
  const userBySubject = async (subject: string) =>
    (await database.client.query("SELECT * FROM users WHERE subject = $1", [subject])).rows;
  const registrations = (subject: string) =>
    count(
      "audit_logs",
      "action = 'memberRegistered' AND entity_id = (SELECT id::text FROM users WHERE subject = $1)",
      [subject],
    );

  test("a never-seen person becomes a member with no initiatives", async () => {
    const oid = crypto.randomUUID();
    const token = await tenant.sign(oid, {
      name: "Tamsin Quarrie",
      email: "tamsin@example.com",
      given_name: "Tamsin",
      family_name: "Quarrie",
    });
    const me = await api.call("/api/v1/me", token);
    expect(me.status).toBe(200);
    expect(me.headers.get("cache-control")).toBe("private, no-store");
    expect(await me.json()).toMatchObject({
      kind: "human",
      displayName: "Tamsin Quarrie",
      email: "tamsin@example.com",
      username: "tamsin",
      agentRank: null,
      isAdmin: false,
    });
    const initiatives = await api.call("/api/v1/me/initiatives", token);
    expect(initiatives.status).toBe(200);
    expect(await initiatives.json()).toEqual({ items: [] });

    const [row] = await userBySubject(oid);
    expect(row).toMatchObject({ role: "member", active: true, agent_rank: null });
    expect(
      await count("organisations", "provider = 'entra' AND external_id = $1", [tenant.tenant]),
    ).toBe(1);
    expect(await count("initiative_memberships", "user_id = $1", [row.id])).toBe(0);
    expect(await registrations(oid)).toBe(1);
    const [audit] = (
      await database.client.query(
        `SELECT actor_id, "actorType" AS actor_type, entity_type, changes FROM audit_logs WHERE entity_id = $1`,
        [row.id],
      )
    ).rows;
    expect(audit).toEqual({
      actor_id: row.id,
      actor_type: "human",
      entity_type: "Member",
      changes: { provider: "entra", role: "member", username: "tamsin" },
    });
  });

  test("tokens for another tenant, audience or client are refused and create nothing", async () => {
    const oid = crypto.randomUUID();
    const users = await count("users");
    const organisations = await count("organisations");
    for (const claims of [
      { tid: crypto.randomUUID() },
      { aud: crypto.randomUUID() },
      { azp: crypto.randomUUID() },
      { scp: "Other.Scope" },
    ]) {
      const response = await api.call("/api/v1/me", await tenant.sign(oid, claims));
      expect(response.status).toBe(401);
    }
    expect(await count("users")).toBe(users);
    expect(await count("organisations")).toBe(organisations);
  });

  test("concurrent first requests from one person register one member", async () => {
    const oid = crypto.randomUUID();
    const token = await tenant.sign(oid, { name: "Bryony Lisle", email: "bryony@example.com" });
    // Hold the organisation row while every request starts, then release them together.
    expect((await api.call("/api/v1/me", await tenant.sign(crypto.randomUUID()))).status).toBe(200);
    await database.client.query("BEGIN");
    const held = await database.client.query(
      "SELECT id FROM organisations WHERE provider = 'entra' AND external_id = $1 FOR UPDATE",
      [tenant.tenant],
    );
    expect(held.rowCount).toBe(1);
    const pending = Promise.all(Array.from({ length: 8 }, () => api.call("/api/v1/me", token)));
    await Bun.sleep(300);
    await database.client.query("COMMIT");
    const responses = await pending;
    const bodies = await Promise.all(responses.map((response) => response.json()));
    expect(responses.map((response) => response.status)).toEqual(Array(8).fill(200));
    expect(new Set(bodies.map((body) => body.userId)).size).toBe(1);
    expect(await userBySubject(oid)).toHaveLength(1);
    expect(await registrations(oid)).toBe(1);
  });

  test("the first sign-ins to a new organisation create it once", async () => {
    const other = await entraTenant();
    const api2 = await startApi(database.url, other.env, other.fetchRemote);
    try {
      const responses = await Promise.all(
        Array.from({ length: 6 }, async () =>
          api2.call("/api/v1/me", await other.sign(crypto.randomUUID(), { name: "Some One" })),
        ),
      );
      expect(responses.map((response) => response.status)).toEqual(Array(6).fill(200));
      expect(await count("organisations", "external_id = $1", [other.tenant])).toBe(1);
      expect(
        await count(
          "users",
          "organisation_id = (SELECT id FROM organisations WHERE external_id = $1)",
          [other.tenant],
        ),
      ).toBe(6);
    } finally {
      await api2.close();
    }
  });

  test("with registration off an unknown person is refused and nothing is created", async () => {
    const oid = crypto.randomUUID();
    const users = await count("users");
    const response = await closed.call(
      "/api/v1/me",
      await tenant.sign(oid, { name: "Osric Fell" }),
    );
    expect(response.status).toBe(403);
    expect((await response.json()).detail).toBe("An active organisation membership is required.");
    expect(await count("users")).toBe(users);
    // A known person still signs in.
    const known = crypto.randomUUID();
    expect((await api.call("/api/v1/me", await tenant.sign(known))).status).toBe(200);
    expect((await closed.call("/api/v1/me", await tenant.sign(known))).status).toBe(200);
  });

  test("a deactivated person stays refused and is never reactivated", async () => {
    const oid = crypto.randomUUID();
    const token = await tenant.sign(oid, { name: "Merrin Oakes" });
    expect((await api.call("/api/v1/me", token)).status).toBe(200);
    await database.client.query("UPDATE users SET active = false WHERE subject = $1", [oid]);
    for (const server of [api, admins]) {
      const response = await server.call(
        "/api/v1/me",
        await tenant.sign(oid, { roles: [adminRole] }),
      );
      expect(response.status).toBe(403);
    }
    const [row] = await userBySubject(oid);
    expect(row).toMatchObject({ active: false, role: "member" });
    expect(await userBySubject(oid)).toHaveLength(1);
    expect(await registrations(oid)).toBe(1);
  });

  test("the Entra admin role decides the role on each request, both ways", async () => {
    const oid = crypto.randomUUID();
    const withRole = await tenant.sign(oid, { name: "Ysolde Penhallow", roles: [adminRole] });
    const without = await tenant.sign(oid, { name: "Ysolde Penhallow", roles: ["Other.Role"] });
    const isAdmin = async (token: string) => {
      const response = await admins.call("/api/v1/me", token);
      expect(response.status).toBe(200);
      return (await response.json()).isAdmin as boolean;
    };
    expect(await isAdmin(withRole)).toBe(true);
    const [registered] = await userBySubject(oid);
    // The provisioned role is untouched; the token's role is kept apart.
    expect(registered).toMatchObject({ role: "member", token_role: "admin" });
    const changes = () =>
      count("audit_logs", "action = 'memberRoleChanged' AND entity_id = $1", [registered.id]);
    expect(await changes()).toBe(0);

    expect(await isAdmin(without)).toBe(false);
    expect((await userBySubject(oid))[0]).toMatchObject({ role: "member", token_role: "member" });
    expect(await changes()).toBe(1);
    // An unchanged role writes nothing.
    const updatedAt = (await userBySubject(oid))[0].updated_at;
    expect(await isAdmin(without)).toBe(false);
    expect((await userBySubject(oid))[0].updated_at).toEqual(updatedAt);
    expect(await changes()).toBe(1);
    expect(await isAdmin(withRole)).toBe(true);
    expect(await changes()).toBe(2);
    // Administrator routes follow the role in the same request.
    const fleet = {
      name: "Example fleet",
      slug: `fleet-${oid.slice(0, 8)}`,
      ircHost: "irc.example.com",
      ircPort: 6697,
      ircTls: true,
    };
    expect((await admins.call("/api/v1/fleets", without, "POST", fleet)).status).toBe(403);
    expect((await admins.call("/api/v1/fleets", withRole, "POST", fleet)).status).toBe(201);
  });

  test("removing ENTRA_ADMIN_ROLE withdraws the grants it made", async () => {
    const oid = crypto.randomUUID();
    const token = await tenant.sign(oid, { name: "Ysolde Kitto", roles: [adminRole] });
    expect((await (await admins.call("/api/v1/me", token)).json()).isAdmin).toBe(true);
    expect((await userBySubject(oid))[0]).toMatchObject({ role: "member", token_role: "admin" });
    // The same token on a server without the setting: the provisioned role applies.
    expect((await (await api.call("/api/v1/me", token)).json()).isAdmin).toBe(false);
    const fleet = {
      name: "Withdrawn fleet",
      slug: `withdrawn-${oid.slice(0, 8)}`,
      ircHost: "irc.example.com",
      ircPort: 6697,
      ircTls: true,
    };
    expect((await api.call("/api/v1/fleets", token, "POST", fleet)).status).toBe(403);
  });

  test("a person whose token role was never recorded gets it on their next request", async () => {
    // As an older instance leaves them: no token role, and here a provisioned administrator.
    const oid = crypto.randomUUID();
    expect((await api.call("/api/v1/me", await tenant.sign(oid))).status).toBe(200);
    await database.client.query(
      "UPDATE users SET role = 'admin', token_role = NULL WHERE subject = $1",
      [oid],
    );
    const without = await tenant.sign(oid, { roles: [] });
    // With the setting, the token decides, and the provisioned role stays as it is.
    expect((await (await admins.call("/api/v1/me", without)).json()).isAdmin).toBe(false);
    expect((await userBySubject(oid))[0]).toMatchObject({ role: "admin", token_role: "member" });
    await database.client.query("UPDATE users SET token_role = NULL WHERE subject = $1", [oid]);
    const holder = await tenant.sign(oid, { roles: [adminRole] });
    expect((await (await admins.call("/api/v1/me", holder)).json()).isAdmin).toBe(true);
    expect((await userBySubject(oid))[0]).toMatchObject({ role: "admin", token_role: "admin" });
    // Without the setting, the provisioned administrator is one again.
    expect((await (await api.call("/api/v1/me", without)).json()).isAdmin).toBe(true);
  });

  test("a token without roles is a member when the admin role is set", async () => {
    const oid = crypto.randomUUID();
    const response = await admins.call("/api/v1/me", await tenant.sign(oid));
    expect(response.status).toBe(200);
    expect((await response.json()).isAdmin).toBe(false);
  });

  test("without ENTRA_ADMIN_ROLE the stored role is left alone", async () => {
    const oid = crypto.randomUUID();
    expect((await api.call("/api/v1/me", await tenant.sign(oid))).status).toBe(200);
    await database.client.query("UPDATE users SET role = 'admin' WHERE subject = $1", [oid]);
    const response = await api.call("/api/v1/me", await tenant.sign(oid, { roles: [] }));
    expect((await response.json()).isAdmin).toBe(true);
    // Registration with the role unset never grants it, whatever the token holds.
    const other = crypto.randomUUID();
    const registered = await api.call(
      "/api/v1/me",
      await tenant.sign(other, { roles: [adminRole] }),
    );
    expect((await registered.json()).isAdmin).toBe(false);
  });

  test("an agent record is never made an administrator", async () => {
    const oid = crypto.randomUUID();
    const owner = crypto.randomUUID();
    expect((await api.call("/api/v1/me", await tenant.sign(owner))).status).toBe(200);
    const [{ id: ownerId, organisation_id: org }] = await userBySubject(owner);
    await database.client.query(
      `INSERT INTO users (organisation_id, subject, display_name, agent_rank, agent_fleet, agent_model, registered_by_id)
       VALUES ($1, $2, 'Orion', 'crew', 'alpha', 'example-model', $3)`,
      [org, oid, ownerId],
    );
    const response = await admins.call(
      "/api/v1/me",
      await tenant.sign(oid, { roles: [adminRole] }),
    );
    expect(response.status).toBe(200);
    expect(await response.json()).toMatchObject({ kind: "agent", isAdmin: false });
    expect((await userBySubject(oid))[0]).toMatchObject({ role: "member", token_role: null });
    // The database refuses an agent a token role, however it is written.
    await expect(
      database.client.query("UPDATE users SET token_role = 'admin' WHERE subject = $1", [oid]),
    ).rejects.toThrow("users_token_role");
  });

  test("the e-mail falls back to preferred_username at registration", async () => {
    const oid = crypto.randomUUID();
    const response = await api.call(
      "/api/v1/me",
      await tenant.sign(oid, { name: "Demelza Rowe", preferred_username: "dr@example.com" }),
    );
    expect(await response.json()).toMatchObject({
      displayName: "Demelza Rowe",
      email: "dr@example.com",
    });
  });
});
