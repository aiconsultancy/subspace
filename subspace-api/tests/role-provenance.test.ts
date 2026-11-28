import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { type RemoteFetch } from "../src/auth";
import { agentKey, deviceToken, type AgentKey } from "./certificates";
import {
  acrossOrganisationLock,
  entraTenant,
  isolatedDatabase,
  startApi,
  testDatabase,
  workosEnvironment,
} from "./entra-stack";

/**
 * Where a request's role comes from, with ENTRA_ADMIN_ROLE configured: a person's own Entra token
 * gives its role; a paired device or a linked account acts with the provisioned role, never above
 * the recorded token role. A request checked again under its organisation's lock keeps the source
 * it signed in with, whatever commits meanwhile.
 */

const deviceAudience = "https://subspace.example.internal";
const adminRole = "Subspace.Admin";

describe.skipIf(!testDatabase)("role provenance", () => {
  let database: Awaited<ReturnType<typeof isolatedDatabase>>;
  let tenant: Awaited<ReturnType<typeof entraTenant>>;
  let workos: Awaited<ReturnType<typeof workosEnvironment>>;
  let api: Awaited<ReturnType<typeof startApi>>;
  let organisationId: string;
  /** A member of the tenant whose device and linked account other members try to remove. */
  let victim: { id: string; credentialId: string; identityId: string };

  beforeAll(async () => {
    database = await isolatedDatabase("role_provenance");
    tenant = await entraTenant();
    workos = await workosEnvironment();
    const fetchRemote: RemoteFetch = async (url, init) =>
      url === workos.jwksUrl
        ? Response.json({ keys: [workos.jwk] })
        : tenant.fetchRemote(url, init);
    api = await startApi(
      database.url,
      {
        ...tenant.env,
        NODE_ENV: "production",
        AUTH_PROVIDERS: "entra,workos,pairing",
        ENTRA_ADMIN_ROLE: adminRole,
        WORKOS_CLIENT_ID: "client_example",
        WORKOS_JWKS_URL: workos.jwksUrl,
        DEVICE_TOKEN_AUDIENCE: deviceAudience,
      },
      fetchRemote,
    );
    const oid = crypto.randomUUID();
    const id = await signIn(oid);
    organisationId = (
      await database.client.query("SELECT organisation_id FROM users WHERE id = $1", [id])
    ).rows[0].organisation_id;
    const device = await pairDevice(oid);
    victim = { id, credentialId: device.credentialId, identityId: await insertLink(id) };
  });
  afterAll(async () => {
    await api?.close();
    await database?.drop();
  });

  /** An Entra token for `oid`, with the administrator app role or without it. */
  const entra = (oid: string, admin = false) =>
    tenant.sign(oid, { roles: admin ? [adminRole] : [], name: `Person ${oid.slice(0, 8)}` });
  const call = async (token: AgentKey | string, path: string, method = "GET", body?: unknown) => {
    const bearer = typeof token === "string" ? token : await deviceToken(token, deviceAudience);
    const response = await api.call(path, bearer, method, body);
    return { status: response.status, body: await response.json() };
  };
  /** Signs `oid` in with their Entra token, which records its role, and returns their user ID. */
  const signIn = async (oid: string, admin = false) => {
    const me = await call(await entra(oid, admin), "/api/v1/me");
    expect(me.status).toBe(200);
    return me.body.userId as string;
  };
  /** A device of the Entra person `oid`, paired with their own invite. */
  const pairDevice = async (oid: string, admin = false) => {
    const issued = await call(await entra(oid, admin), "/api/v1/me/pairing/invites", "POST", {});
    expect(issued.status).toBe(201);
    const key = agentKey();
    const response = await api.call("/api/v1/pairing/redeem", undefined, "POST", {
      invite: issued.body.invite,
      certificate: key.certificate,
    });
    expect(response.status).toBe(201);
    return { key, credentialId: (await response.json()).credentialId as string };
  };
  /** Links a WorkOS account to the member directly, and returns its link's ID. */
  const insertLink = async (userId: string, subject = `user_${crypto.randomUUID()}`) =>
    (
      await database.client.query(
        "INSERT INTO member_identities (organisation_id, user_id, provider, external_organisation, subject) VALUES ($1, $2, 'workos', 'org_people', $3) RETURNING id",
        [organisationId, userId, subject],
      )
    ).rows[0].id as string;
  const setRole = (userId: string, role: "admin" | "member") =>
    database.client.query("UPDATE users SET role = $2 WHERE id = $1", [userId, role]);
  /**
   * Records an Entra token role as the person's sign-in does. Across the lock it is written
   * directly: the sign-in itself takes the organisation's lock, so it would queue behind the
   * request, whereas this is a sign-in that committed between the request's and its recheck.
   */
  const setTokenRole = (userId: string, role: "admin" | "member") =>
    database.client.query("UPDATE users SET token_role = $2 WHERE id = $1", [userId, role]);
  const victimRoutes = () => [
    `/api/v1/members/${victim.id}/devices/${victim.credentialId}`,
    `/api/v1/members/${victim.id}/identities/${victim.identityId}`,
  ];
  /** The victim's device and linked account are as they were. */
  const victimUntouched = async () => {
    const device = await database.client.query(
      "SELECT revoked_at FROM agent_credentials WHERE id = $1",
      [victim.credentialId],
    );
    expect(device.rows[0].revoked_at).toBeNull();
    const link = await database.client.query("SELECT id FROM member_identities WHERE id = $1", [
      victim.identityId,
    ]);
    expect(link.rows).toHaveLength(1);
    const audits = await database.client.query(
      "SELECT action FROM audit_logs WHERE entity_id IN ($1, $2) AND action IN ('deviceCredentialRevoked', 'memberIdentityUnlinked')",
      [victim.credentialId, victim.identityId],
    );
    expect(audits.rows).toEqual([]);
  };
  /** `request`, held before its locked recheck while `during` commits. */
  const across = (request: () => Promise<{ status: number }>, during: () => Promise<unknown>) =>
    acrossOrganisationLock(database.url, organisationId, request, during);

  test("a linked account's request never takes the administrator role a native sign-in records meanwhile", async () => {
    const oid = crypto.randomUUID();
    const person = await signIn(oid);
    const subject = `user_${crypto.randomUUID()}`;
    await insertLink(person, subject);
    const linked = await workos.sign(subject, "org_people");
    for (const path of victimRoutes()) {
      // The person's own Entra token is a member's again, as their linked account is.
      await signIn(oid);
      const refused = await across(
        () => call(linked, path, "DELETE"),
        () => setTokenRole(person, "admin"),
      );
      expect(refused.status).toBe(403);
      await victimUntouched();
    }
  });

  test("a native Entra request loses the administrator role its token drops meanwhile", async () => {
    const oid = crypto.randomUUID();
    const person = await signIn(oid, true);
    const token = await entra(oid, true);
    for (const path of victimRoutes()) {
      await signIn(oid, true);
      const refused = await across(
        () => call(token, path, "DELETE"),
        () => setTokenRole(person, "member"),
      );
      expect(refused.status).toBe(403);
      await victimUntouched();
    }
  });

  test("a native Entra request never takes the administrator role another sign-in records meanwhile", async () => {
    const oid = crypto.randomUUID();
    const person = await signIn(oid);
    // Its own token is a member's: another sign-in of the same person, whose token carries the
    // app role, records it between this request's sign-in and its recheck.
    const token = await entra(oid);
    for (const path of victimRoutes()) {
      await signIn(oid);
      const refused = await across(
        () => call(token, path, "DELETE"),
        () => setTokenRole(person, "admin"),
      );
      expect(refused.status).toBe(403);
      await victimUntouched();
    }
  });

  test("no request of a member binds a provider organisation with the administrator role recorded meanwhile", async () => {
    const oid = crypto.randomUUID();
    const person = await signIn(oid);
    const { key } = await pairDevice(oid);
    // A WorkOS organisation linked nowhere yet: its first link here binds it, which needs an
    // administrator.
    const providerOrganisation = `org_${crypto.randomUUID().replaceAll("-", "")}`;
    const link = async (bearer: AgentKey | string) =>
      call(bearer, "/api/v1/me/identities", "POST", {
        token: await workos.sign(`user_${crypto.randomUUID()}`, providerOrganisation),
      });
    const nothingLinked = async () => {
      const links = await database.client.query(
        "SELECT id FROM member_identities WHERE external_organisation = $1",
        [providerOrganisation],
      );
      expect(links.rows).toEqual([]);
      const audits = await database.client.query(
        "SELECT id FROM audit_logs WHERE action = 'memberIdentityLinked' AND changes->>'memberId' = $1",
        [person],
      );
      expect(audits.rows).toEqual([]);
    };
    // Their own Entra token is a member's: linking needs a paired device's signature, so it is
    // refused before the lock, whatever its role.
    expect((await link(await entra(oid))).status).toBe(403);
    await nothingLinked();
    // Their device, held before its recheck while a sign-in whose token carries the app role
    // records it: a device acts with the provisioned role, a member's, never the token's.
    const refused = await across(
      () => link(key),
      () => setTokenRole(person, "admin"),
    );
    expect(refused.status).toBe(403);
    await nothingLinked();
  });

  test("a device's and a linked account's request lose the provisioned role a demotion removes meanwhile", async () => {
    const oid = crypto.randomUUID();
    const person = await signIn(oid, true);
    await setRole(person, "admin");
    const { key } = await pairDevice(oid, true);
    const subject = `user_${crypto.randomUUID()}`;
    await insertLink(person, subject);
    const linked = await workos.sign(subject, "org_people");
    for (const token of [key, linked])
      for (const path of victimRoutes()) {
        await setRole(person, "admin");
        const refused = await across(
          () => call(token, path, "DELETE"),
          () => setRole(person, "member"),
        );
        expect(refused.status).toBe(403);
        await victimUntouched();
      }
  });

  test("a device and a linked account stop acting as an administrator once the token role drops", async () => {
    const oid = crypto.randomUUID();
    const person = await signIn(oid, true);
    await setRole(person, "admin");
    const { key } = await pairDevice(oid, true);
    const subject = `user_${crypto.randomUUID()}`;
    await insertLink(person, subject);
    const linked = async () => workos.sign(subject, "org_people");
    expect((await call(key, "/api/v1/me")).body.isAdmin).toBe(true);
    expect((await call(await linked(), "/api/v1/me")).body.isAdmin).toBe(true);
    const issued = await call(key, "/api/v1/pairing/invites", "POST", { displayName: "Kitto" });
    expect(issued.status).toBe(201);

    // The provider no longer makes them an administrator: nor does their device or linked account.
    await signIn(oid);
    for (const token of [key, await linked()]) {
      expect((await call(token, "/api/v1/me")).body.isAdmin).toBe(false);
      const invite = await call(token, "/api/v1/pairing/invites", "POST", { displayName: "Ruan" });
      expect(invite.status).toBe(403);
      for (const path of victimRoutes())
        expect((await call(token, path, "DELETE")).status).toBe(403);
    }
    await victimUntouched();
    // Nor does the token's role alone make the device an administrator's, without the provisioned one.
    await signIn(oid, true);
    expect((await call(key, "/api/v1/me")).body.isAdmin).toBe(true);
    await setRole(person, "member");
    expect((await call(key, "/api/v1/me")).body.isAdmin).toBe(false);
    expect((await call(await entra(oid, true), "/api/v1/me")).body.isAdmin).toBe(true);
  });
});
