import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { eq } from "drizzle-orm";
import { Effect, Exit, Layer } from "effect";
import { OpenApi } from "effect/http-api";
import { documentedApi } from "../src/api";
import { type RemoteFetch } from "../src/auth";
import { Database, databaseLayer } from "../src/db";
import { provisionMember } from "../src/registration";
import { confirmCaller } from "../src/people";
import { pairingInvitePrefix } from "../src/pairing-invites";
import { bootstrapOwner } from "../src/pairing-owner";
import { problemTypes } from "../src/problems";
import { users } from "../src/schema";
import { signedInMember, type SignInPolicy } from "../src/registration";
import { agentKey, deviceToken, type AgentKey } from "./certificates";
import {
  entraTenant,
  isolatedDatabase,
  startApi,
  testDatabase,
  workosEnvironment,
} from "./entra-stack";

const deviceAudience = "https://subspace.example.internal";
const hookToken = "an-example-hook-token-of-at-least-32-bytes";
const ircServer = { host: "irc.example.com", port: 6697 };

test("device, invite and linked account routes are documented", () => {
  const document = OpenApi.fromApi(documentedApi) as { paths: Record<string, unknown> };
  for (const path of [
    "/api/v1/pairing/invites",
    "/api/v1/me/devices",
    "/api/v1/me/devices/{credentialId}",
    "/api/v1/members/{memberId}/devices",
    "/api/v1/members/{memberId}/devices/{credentialId}",
    "/api/v1/me/identities",
    "/api/v1/me/identities/{identityId}",
    "/api/v1/members/{memberId}/identities",
    "/api/v1/members/{memberId}/identities/{identityId}",
  ])
    expect(Object.keys(document.paths)).toContain(path);
  // Linking's problems are named by their full type, as the API returns them.
  const linking = document.paths["/api/v1/me/identities"] as { post: { description: string } };
  expect(linking.post.description).toContain(problemTypes.identityTokenUnusable);
  expect(linking.post.description).toContain(problemTypes.identityConflict);
});

describe.skipIf(!testDatabase)("devices, invites, IRC and linked accounts", () => {
  let database: Awaited<ReturnType<typeof isolatedDatabase>>;
  let tenant: Awaited<ReturnType<typeof entraTenant>>;
  let workos: Awaited<ReturnType<typeof workosEnvironment>>;
  let api: Awaited<ReturnType<typeof startApi>>;
  const organisation = "example";
  const logged: string[] = [];
  const restore: (() => void)[] = [];

  /** The owner, an administrator, with one paired device. */
  let owner: { id: string; key: AgentKey; credentialId: string };

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
    database = await isolatedDatabase("devices");
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
        WORKOS_CLIENT_ID: "client_example",
        WORKOS_JWKS_URL: workos.jwksUrl,
        DEVICE_TOKEN_AUDIENCE: deviceAudience,
        IRC_CREDENTIAL_SECRET: "an example credential secret of at least 32 bytes",
        IRC_AUTH_HOOK_TOKEN: hookToken,
        IRC_AUTH_ORGANISATION: `pairing:${organisation}`,
        IRC_AUTH_SERVERS: "irc.example.com:6697",
      },
      fetchRemote,
    );
    const { invite, owner: created } = await bootstrap(organisation);
    const key = agentKey();
    const paired = await redeem(invite.token, key);
    owner = { id: created.id, key, credentialId: paired.credentialId };
  });
  afterAll(async () => {
    await api?.close();
    await database?.drop();
    for (const undo of restore) undo();
  });

  const bootstrap = (externalOrganisation: string, name = "Tamsin Pryor") =>
    Effect.runPromise(
      bootstrapOwner({
        externalOrganisation,
        displayName: name,
        email: "tamsin@example.com",
        lifetime: 3600,
        reserved: new Set(),
      }).pipe(Effect.provide(databaseLayer(database.url, 2))),
    );
  const redeem = async (invite: string, key: AgentKey) => {
    const response = await api.call("/api/v1/pairing/redeem", undefined, "POST", {
      invite,
      certificate: key.certificate,
    });
    expect(response.status).toBe(201);
    return (await response.json()) as { credentialId: string; memberId: string };
  };
  const signed = (key: AgentKey) => deviceToken(key, deviceAudience);
  const call = async (key: AgentKey | string, path: string, method = "GET", body?: unknown) => {
    const token = typeof key === "string" ? key : await signed(key);
    const response = await api.call(path, token, method, body);
    return { status: response.status, headers: response.headers, body: await response.json() };
  };
  const invite = (body: unknown, key = owner.key) =>
    call(key, "/api/v1/pairing/invites", "POST", body);
  /** An invite that pairs another device of the person who signs in with `key`. */
  const ownInvite = (key: AgentKey | string, body: unknown = {}) =>
    call(key, "/api/v1/me/pairing/invites", "POST", body);
  /** Another device of the person who signs in with `key`, paired with their own invite. */
  const anotherDevice = async (key: AgentKey) => {
    const issued = await ownInvite(key);
    expect(issued.status).toBe(201);
    const device = agentKey();
    return { key: device, ...(await redeem(issued.body.invite, device)) };
  };
  /** A new person invited by the owner, with one paired device. */
  const invitedPerson = async (displayName: string) => {
    const issued = await invite({ displayName });
    expect(issued.status).toBe(201);
    const key = agentKey();
    const paired = await redeem(issued.body.invite, key);
    return { id: issued.body.memberId as string, key, credentialId: paired.credentialId };
  };
  const hook = async (accountName: string, passphrase: string) => {
    const response = await api.call("/api/v1/irc/authenticate", hookToken, "POST", {
      accountName,
      passphrase,
    });
    return (await response.json()) as { success: boolean; accountName?: string };
  };
  const ircPassword = async (key: AgentKey | string) => {
    const issued = await call(key, "/api/v1/me/irc-credential", "POST", ircServer);
    expect(issued.status).toBe(200);
    return issued.body as { account: string; password: string };
  };
  const audits = async (action: string, entityId: string) =>
    (
      await database.client.query(
        "SELECT changes, summary FROM audit_logs WHERE action = $1 AND entity_id = $2",
        [action, entityId],
      )
    ).rows;

  describe("administrator invites", () => {
    test("an administrator invites a new person, who pairs a device and signs in", async () => {
      const issued = await invite({ displayName: "Corin Tarrowmere", email: "corin@example.com" });
      expect(issued.status).toBe(201);
      expect(issued.headers.get("cache-control")).toBe("private, no-store");
      expect(issued.body.memberCreated).toBe(true);
      expect(issued.body.invite.startsWith(pairingInvitePrefix)).toBe(true);
      const lifetime = Date.parse(issued.body.expiresAt) - Date.now();
      expect(lifetime).toBeGreaterThan(3590_000);
      expect(lifetime).toBeLessThanOrEqual(3600_000);

      const key = agentKey("p256");
      const paired = await redeem(issued.body.invite, key);
      expect(paired.memberId).toBe(issued.body.memberId);
      const me = await call(key, "/api/v1/me");
      expect(me.status).toBe(200);
      expect(me.body).toMatchObject({
        userId: issued.body.memberId,
        displayName: "Corin Tarrowmere",
        username: "corin",
        isAdmin: false,
      });
      // Single use, and neither stored, audited nor logged.
      const again = await api.call("/api/v1/pairing/redeem", undefined, "POST", {
        invite: issued.body.invite,
        certificate: agentKey().certificate,
      });
      expect(again.status).toBe(401);
      expect((await again.json()).type).toBe(problemTypes.pairingInviteUnusable);
      const [stored] = (
        await database.client.query("SELECT * FROM pairing_invites WHERE id = $1", [
          issued.body.inviteId,
        ])
      ).rows;
      expect(stored.created_by_id).toBe(owner.id);
      expect(JSON.stringify(stored)).not.toContain(issued.body.invite);
      const audit = await database.client.query("SELECT changes, summary FROM audit_logs");
      expect(JSON.stringify(audit.rows)).not.toContain(issued.body.invite);
      expect(logged.join("\n")).not.toContain(issued.body.invite);
      expect(await audits("pairingInviteCreated", issued.body.inviteId)).toHaveLength(1);
      expect(await audits("memberRegistered", issued.body.memberId)).toHaveLength(1);
    });

    test("an administrator invites an existing person only when they have no way to sign in", async () => {
      const person = await invitedPerson("Esme Halloran");
      // With a paired device, the person pairs another device themselves.
      const refused = await invite({ memberId: person.id });
      expect(refused.status).toBe(409);
      expect(refused.body.type).toBe(problemTypes.memberSignsIn);
      // Once their only device is revoked, an administrator's invite pairs a new one, and a newer
      // invite replaces a pending one.
      await call(
        owner.key,
        `/api/v1/members/${person.id}/devices/${person.credentialId}`,
        "DELETE",
      );
      const first = await invite({ memberId: person.id, expiresInSeconds: 300 });
      expect(first.status).toBe(201);
      expect(first.body).toMatchObject({ memberId: person.id, memberCreated: false });
      expect(Date.parse(first.body.expiresAt) - Date.now()).toBeLessThanOrEqual(300_000);
      const second = await invite({ memberId: person.id });
      expect(second.status).toBe(201);
      expect(await audits("pairingInviteRevoked", first.body.inviteId)).toHaveLength(1);
      const stale = await api.call("/api/v1/pairing/redeem", undefined, "POST", {
        invite: first.body.invite,
        certificate: agentKey().certificate,
      });
      expect(stale.status).toBe(401);
      const laptop = agentKey();
      expect((await redeem(second.body.invite, laptop)).memberId).toBe(person.id);
      expect((await call(laptop, "/api/v1/me")).status).toBe(200);
      expect((await invite({ memberId: person.id })).body.type).toBe(problemTypes.memberSignsIn);
    });

    test("a person with a linked provider account is never invited by an administrator", async () => {
      const person = await invitedPerson("Merryn Coad");
      await call(
        owner.key,
        `/api/v1/members/${person.id}/devices/${person.credentialId}`,
        "DELETE",
      );
      await database.client.query(
        "INSERT INTO member_identities (organisation_id, user_id, provider, external_organisation, subject) SELECT organisation_id, id, 'workos', 'org_linked', 'user_merryn' FROM users WHERE id = $1",
        [person.id],
      );
      const refused = await invite({ memberId: person.id });
      expect(refused.status).toBe(409);
      expect(refused.body.type).toBe(problemTypes.memberSignsIn);
      // The owner, who signs in, is refused too.
      expect((await invite({ memberId: owner.id })).body.type).toBe(problemTypes.memberSignsIn);
    });

    test("a request is checked again under the lock: role, membership and signing device", async () => {
      const person = await invitedPerson("Yestin Penrose");
      // The caller as a request saw them when it signed in: an administrator.
      const confirm = (keyCredential?: { id: string; scopes: ("api" | "irc")[] }) =>
        Effect.runPromise(
          Effect.gen(function* () {
            const db = yield* Database;
            const [member] = yield* db.select().from(users).where(eq(users.id, person.id));
            return yield* db.transaction((tx) =>
              confirmCaller(tx, {
                ...member!,
                role: "admin",
                roleSource: "provisioned",
                tokenGrant: null,
                tokenExpiresAt: 0,
                keyCredential,
              }),
            );
          }).pipe(Effect.provide(databaseLayer(database.url, 2)), Effect.exit),
        );
      const device = { id: person.credentialId, scopes: ["api", "irc"] as ("api" | "irc")[] };
      const confirmed = await confirm(device);
      expect(Exit.isSuccess(confirmed) && confirmed.value.role).toBe("member");
      await database.client.query("UPDATE agent_credentials SET revoked_at = now() WHERE id = $1", [
        person.credentialId,
      ]);
      expect(Exit.isFailure(await confirm(device))).toBe(true);
      await database.client.query("UPDATE users SET active = false WHERE id = $1", [person.id]);
      expect(Exit.isFailure(await confirm())).toBe(true);
    });

    test("only an administrator invites, for an active person of the organisation", async () => {
      const person = await invitedPerson("Bryn Askew");
      const refused = await invite({ displayName: "Nobody" }, person.key);
      expect(refused.status).toBe(403);
      expect((await invite({ memberId: person.id }, person.key)).status).toBe(403);
      expect((await invite({ memberId: crypto.randomUUID() })).status).toBe(404);
      for (const expiresInSeconds of [299, 86_401])
        expect((await invite({ memberId: person.id, expiresInSeconds })).status).toBe(400);
      expect((await invite({ memberId: person.id, displayName: "Both" })).status).toBe(400);
      expect((await invite({ displayName: "Owner", role: "owner" })).status).toBe(400);
      await database.client.query("UPDATE users SET active = false WHERE id = $1", [person.id]);
      expect((await invite({ memberId: person.id })).status).toBe(409);
      await database.client.query("UPDATE users SET active = true WHERE id = $1", [person.id]);
    });
  });

  describe("own invites and what a device issued", () => {
    test("a person pairs another device with their own invite, which names the device", async () => {
      const person = await invitedPerson("Nessa Polglaze");
      const first = await ownInvite(person.key, { expiresInSeconds: 600 });
      expect(first.status).toBe(201);
      expect(first.headers.get("cache-control")).toBe("private, no-store");
      expect(first.body).toMatchObject({ memberId: person.id, memberCreated: false });
      // A newer one replaces it.
      const second = await ownInvite(person.key);
      expect(await audits("pairingInviteRevoked", first.body.inviteId)).toHaveLength(1);
      const laptop = agentKey();
      const paired = await redeem(second.body.invite, laptop);
      const listed = await call(laptop, "/api/v1/me/devices");
      const device = listed.body.items.find(
        (item: { id: string }) => item.id === paired.credentialId,
      );
      expect(device).toMatchObject({
        pairingInviteId: second.body.inviteId,
        invitedByUserId: person.id,
        invitedByDeviceId: person.credentialId,
        current: true,
      });
      // The device the owner's invite paired names the owner and the owner's device.
      const original = listed.body.items.find(
        (item: { id: string }) => item.id === person.credentialId,
      );
      expect(original).toMatchObject({
        invitedByUserId: owner.id,
        invitedByDeviceId: owner.credentialId,
      });
      expect((await ownInvite(person.key, { memberId: owner.id })).status).toBe(400);
    });

    test("revoking a device revokes what it issued, and what that issued in turn", async () => {
      const admin = await invitedPerson("Pascoe Hendra");
      await database.client.query("UPDATE users SET role = 'admin' WHERE id = $1", [admin.id]);
      // The administrator's device invites a person, whose device pairs another of theirs, and
      // issues an invite that is still pending.
      const issued = await invite({ displayName: "Rosen Trethewey" }, admin.key);
      expect(issued.status).toBe(201);
      const desk = agentKey();
      const paired = await redeem(issued.body.invite, desk);
      const laptop = await anotherDevice(desk);
      const pending = await invite({ displayName: "Senara Bolitho" }, admin.key);
      expect(pending.status).toBe(201);
      const listed = await call(owner.key, `/api/v1/members/${paired.memberId}/devices`);
      expect(
        listed.body.items.find((item: { id: string }) => item.id === paired.credentialId),
      ).toMatchObject({
        invitedByUserId: admin.id,
        invitedByDeviceId: admin.credentialId,
      });

      const revoked = await call(
        owner.key,
        `/api/v1/members/${admin.id}/devices/${admin.credentialId}`,
        "DELETE",
      );
      expect(revoked.status).toBe(200);
      expect((await call(desk, "/api/v1/me")).status).toBe(401);
      expect((await call(laptop.key, "/api/v1/me")).status).toBe(401);
      const stale = await api.call("/api/v1/pairing/redeem", undefined, "POST", {
        invite: pending.body.invite,
        certificate: agentKey().certificate,
      });
      expect(stale.status).toBe(401);
      const [deskAudit] = await audits("deviceCredentialRevoked", paired.credentialId);
      expect(deskAudit.changes).toMatchObject({
        memberId: paired.memberId,
        issuingDeviceId: admin.credentialId,
      });
      const [laptopAudit] = await audits("deviceCredentialRevoked", laptop.credentialId);
      expect(laptopAudit.changes).toMatchObject({ issuingDeviceId: paired.credentialId });
      expect(await audits("pairingInviteRevoked", pending.body.inviteId)).toHaveLength(1);
      // The owner, who revoked them, and the owner's device are untouched.
      expect((await call(owner.key, "/api/v1/me")).status).toBe(200);
    });
  });

  describe("pending invites", () => {
    test("an administrator lists and revokes pending invites; others may not", async () => {
      const issued = await invite({ displayName: "Tegen Angwin" });
      const listed = await call(owner.key, "/api/v1/pairing/invites");
      expect(listed.status).toBe(200);
      const found = listed.body.items.find(
        (item: { id: string }) => item.id === issued.body.inviteId,
      );
      expect(found).toEqual({
        id: issued.body.inviteId,
        memberId: issued.body.memberId,
        createdAt: expect.any(String),
        expiresAt: issued.body.expiresAt,
        redeemedAt: null,
        revokedAt: null,
        createdByUserId: owner.id,
        createdByDeviceId: owner.credentialId,
      });
      expect(JSON.stringify(listed.body)).not.toContain(issued.body.invite);
      const person = await invitedPerson("Wenna Jago");
      expect((await call(person.key, "/api/v1/pairing/invites")).status).toBe(403);
      const path = `/api/v1/pairing/invites/${issued.body.inviteId}`;
      expect((await call(person.key, path, "DELETE")).status).toBe(403);

      const revoked = await call(owner.key, path, "DELETE");
      expect(revoked.status).toBe(200);
      expect(typeof revoked.body.revokedAt).toBe("string");
      expect(await audits("pairingInviteRevoked", issued.body.inviteId)).toHaveLength(1);
      const stale = await api.call("/api/v1/pairing/redeem", undefined, "POST", {
        invite: issued.body.invite,
        certificate: agentKey().certificate,
      });
      expect(stale.status).toBe(401);
      // Again, unchanged; no longer listed; an unknown invite is not found.
      expect((await call(owner.key, path, "DELETE")).body.revokedAt).toBe(revoked.body.revokedAt);
      expect(await audits("pairingInviteRevoked", issued.body.inviteId)).toHaveLength(1);
      const after = await call(owner.key, "/api/v1/pairing/invites");
      expect(after.body.items.map((item: { id: string }) => item.id)).not.toContain(
        issued.body.inviteId,
      );
      expect(
        (await call(owner.key, `/api/v1/pairing/invites/${crypto.randomUUID()}`, "DELETE")).status,
      ).toBe(404);
    });
  });

  describe("devices", () => {
    test("a person lists their devices and revokes one; the others keep working", async () => {
      const person = await invitedPerson("Hollis Penhale");
      const { key: laptop, ...second } = await anotherDevice(person.key);

      const listed = await call(person.key, "/api/v1/me/devices");
      expect(listed.status).toBe(200);
      expect(listed.body.items).toHaveLength(2);
      const current = listed.body.items.find((item: { current: boolean }) => item.current);
      expect(current).toMatchObject({
        id: person.credentialId,
        memberId: person.id,
        scopes: ["api", "irc"],
        revokedAt: null,
      });

      const revoked = await call(person.key, `/api/v1/me/devices/${second.credentialId}`, "DELETE");
      expect(revoked.status).toBe(200);
      expect(revoked.body).toMatchObject({ id: second.credentialId, revokedByUserId: person.id });
      expect(typeof revoked.body.revokedAt).toBe("string");
      expect((await call(laptop, "/api/v1/me")).status).toBe(401);
      expect((await call(person.key, "/api/v1/me")).status).toBe(200);
      // Again, unchanged; and audited once.
      const again = await call(person.key, `/api/v1/me/devices/${second.credentialId}`, "DELETE");
      expect(again.body.revokedAt).toBe(revoked.body.revokedAt);
      expect(await audits("deviceCredentialRevoked", second.credentialId)).toHaveLength(1);
      // Someone else's device is not found among the caller's.
      expect(
        (await call(person.key, `/api/v1/me/devices/${owner.credentialId}`, "DELETE")).status,
      ).toBe(404);
    });

    test("an administrator lists and revokes a person's devices; others may not", async () => {
      const person = await invitedPerson("Ione Trevail");
      const listed = await call(owner.key, `/api/v1/members/${person.id}/devices`);
      expect(listed.status).toBe(200);
      expect(listed.body.items.map((item: { id: string }) => item.id)).toEqual([
        person.credentialId,
      ]);
      expect(listed.body.items[0].current).toBe(false);
      expect((await call(person.key, `/api/v1/members/${owner.id}/devices`)).status).toBe(403);
      expect(
        (
          await call(
            person.key,
            `/api/v1/members/${owner.id}/devices/${owner.credentialId}`,
            "DELETE",
          )
        ).status,
      ).toBe(403);
      expect((await call(owner.key, `/api/v1/members/${crypto.randomUUID()}/devices`)).status).toBe(
        404,
      );

      const revoked = await call(
        owner.key,
        `/api/v1/members/${person.id}/devices/${person.credentialId}`,
        "DELETE",
      );
      expect(revoked.status).toBe(200);
      expect(revoked.body.revokedByUserId).toBe(owner.id);
      expect((await call(person.key, "/api/v1/me")).status).toBe(401);
      expect((await call(owner.key, "/api/v1/me")).status).toBe(200);
      const [audit] = await audits("deviceCredentialRevoked", person.credentialId);
      expect(audit.changes).toMatchObject({ memberId: person.id });
    });

    test("the agent credential routes never show a device", async () => {
      const person = await invitedPerson("Jory Nance");
      expect((await call(owner.key, `/api/v1/agents/${person.id}/credentials`)).status).toBe(404);
    });
  });

  describe("IRC", () => {
    test("a paired device gets an IRC password bound to it, refused once it is revoked", async () => {
      const person = await invitedPerson("Kitto Rowe");
      const { key: laptop, ...second } = await anotherDevice(person.key);

      const desk = await ircPassword(person.key);
      expect(desk.account).toBe("kitto");
      expect(desk.password).toMatch(/^sirc2\.[A-Za-z0-9_-]{96}$/u);
      const travel = await ircPassword(laptop);
      expect(await hook("kitto", desk.password)).toEqual({ success: true, accountName: "kitto" });
      expect(await hook("kitto@laptop", travel.password)).toEqual({
        success: true,
        accountName: "kitto",
      });

      const revoked = await call(person.key, `/api/v1/me/devices/${second.credentialId}`, "DELETE");
      expect(revoked.status).toBe(200);
      // The revoked device's password, issued before, no longer signs in; the other one does.
      expect(await hook("kitto", travel.password)).toEqual({ success: false });
      expect(await hook("kitto", desk.password)).toEqual({ success: true, accountName: "kitto" });
      // Nor can it ask for a new one.
      expect((await call(laptop, "/api/v1/me/irc-credential", "POST", ircServer)).status).toBe(401);
    });
  });

  describe("linked accounts", () => {
    /** An Entra token of a member of the tenant, whose acct claim says so, as a first link needs. */
    const entraOf = async (oid: string, claims: Record<string, unknown> = {}) =>
      tenant.sign(oid, { acct: 0, ...claims });
    const link = (key: AgentKey | string, token: string) =>
      call(key, "/api/v1/me/identities", "POST", { token });

    test("the owner links Entra and WorkOS accounts, which then sign in as the owner", async () => {
      const oid = crypto.randomUUID();
      // A paired person may not open the organisation to the tenant: an administrator does.
      const person = await invitedPerson("Lowen Carne");
      const early = await link(person.key, await entraOf(crypto.randomUUID()));
      expect(early.status).toBe(403);

      const linked = await link(owner.key, await entraOf(oid));
      expect(linked.status).toBe(201);
      expect(linked.body).toMatchObject({
        memberId: owner.id,
        provider: "entra",
        externalOrganisation: tenant.tenant,
        subject: oid,
      });
      // Linking it again returns the same link.
      const again = await link(owner.key, await entraOf(oid));
      expect(again.status).toBe(201);
      expect(again.body.id).toBe(linked.body.id);
      const [audit] = await audits("memberIdentityLinked", linked.body.id);
      expect(audit.changes).toMatchObject({ credentialId: owner.credentialId, subject: oid });

      // The Entra account signs in as the owner, with the owner's role and username, even when
      // the token carries no administrator role; and its IRC account is the owner's.
      const entra = await entraOf(oid, { roles: [] });
      const me = await call(entra, "/api/v1/me");
      expect(me.status).toBe(200);
      expect(me.body).toMatchObject({ userId: owner.id, username: "tamsin", isAdmin: true });
      const irc = await ircPassword(entra);
      expect(irc.account).toBe("tamsin");
      expect(irc.password).toMatch(/^sirc1\./u);
      expect(await hook("tamsin", irc.password)).toEqual({ success: true, accountName: "tamsin" });
      // No organisation was registered for the tenant.
      const tenants = await database.client.query(
        "SELECT id FROM organisations WHERE provider = 'entra'",
      );
      expect(tenants.rows).toEqual([]);

      const workosLink = await link(owner.key, await workos.sign("user_owner", "org_example"));
      expect(workosLink.status).toBe(201);
      expect(workosLink.body).toMatchObject({ provider: "workos", subject: "user_owner" });
      const viaWorkos = await call(await workos.sign("user_owner", "org_example"), "/api/v1/me");
      expect(viaWorkos.body).toMatchObject({ userId: owner.id, username: "tamsin" });
      const mine = await call(owner.key, "/api/v1/me/identities");
      expect(mine.body.items.map((item: { provider: string }) => item.provider)).toEqual([
        "entra",
        "workos",
      ]);

      // Once the tenant is linked here, members link their own accounts of it.
      const personOid = crypto.randomUUID();
      expect((await link(person.key, await entraOf(personOid))).status).toBe(201);
      expect((await call(await entraOf(personOid), "/api/v1/me")).body.userId).toBe(person.id);
      // An unknown person of the tenant is not registered in an organisation of its own.
      const stranger = await call(await entraOf(crypto.randomUUID()), "/api/v1/me");
      expect(stranger.status).toBe(403);
      expect(
        (await database.client.query("SELECT id FROM organisations WHERE provider = 'entra'")).rows,
      ).toEqual([]);
    });

    test("an account never links to a second member, nor a member to a second account", async () => {
      const person = await invitedPerson("Merryn Tozer");
      const ownersEntra = (
        await database.client.query(
          "SELECT subject FROM member_identities WHERE user_id = $1 AND provider = 'entra'",
          [owner.id],
        )
      ).rows[0].subject as string;
      // Nor does the membership command register a linked account as a member of its own.
      const provisioned = await Effect.runPromiseExit(
        provisionMember(
          { provider: "entra", externalOrganisation: tenant.tenant, subject: ownersEntra },
          { displayName: "Second Tamsin", email: null },
          "admin",
        ).pipe(Effect.provide(databaseLayer(database.url, 2))),
      );
      expect(Exit.isFailure(provisioned)).toBe(true);
      expect((await call(await entraOf(ownersEntra), "/api/v1/me")).body.userId).toBe(owner.id);
      const taken = await link(person.key, await entraOf(ownersEntra));
      expect(taken.status).toBe(409);
      expect(taken.body.type).toBe(problemTypes.identityConflict);
      const second = await link(owner.key, await entraOf(crypto.randomUUID()));
      expect(second.status).toBe(409);
      expect(second.body.type).toBe(problemTypes.identityConflict);

      // A stale, foreign or malformed token, or a key token, never links anything.
      for (const token of [
        await tenant.sign(
          crypto.randomUUID(),
          {},
          { issuedAt: Math.floor(Date.now() / 1000) - 601 },
        ),
        await workos.sign("user_stale", "org_example", 601),
        await entraOf(crypto.randomUUID(), { tid: crypto.randomUUID() }),
        "not a token",
        await signed(person.key),
      ]) {
        const refused = await link(person.key, token);
        expect(refused.status).toBe(400);
        expect(refused.body.type).toBe(problemTypes.identityTokenUnusable);
      }
      // A body with a field the route does not take is refused before anything is read.
      const extra = await call(person.key, "/api/v1/me/identities", "POST", {
        token: await entraOf(crypto.randomUUID()),
        memberId: owner.id,
      });
      expect(extra.status).toBe(400);
      // Only a paired device links: not a provider sign-in.
      const viaEntra = await entraOf(ownersEntra);
      expect((await link(viaEntra, await entraOf(crypto.randomUUID()))).status).toBe(403);
    });

    test("an account registered in its own organisation, or one linked elsewhere, is refused", async () => {
      const [{ id: legacy }] = (
        await database.client.query(
          "INSERT INTO organisations (provider, external_id) VALUES ('workos', 'org_legacy') RETURNING id",
        )
      ).rows;
      await database.client.query(
        "INSERT INTO users (organisation_id, subject, display_name, role) VALUES ($1, 'user_legacy', 'Legacy', 'admin')",
        [legacy],
      );
      const registered = await link(owner.key, await workos.sign("user_legacy", "org_legacy"));
      expect(registered.status).toBe(409);
      // The owner holds a WorkOS account already; another administrator links this one.
      const admin = await invitedPerson("Nessa Polkinghorne");
      await database.client.query("UPDATE users SET role = 'admin' WHERE id = $1", [admin.id]);
      const sameOrganisation = await link(admin.key, await workos.sign("user_other", "org_legacy"));
      expect(sameOrganisation.status).toBe(409);
      expect(sameOrganisation.body.type).toBe(problemTypes.identityConflict);
      expect(
        (await call(await workos.sign("user_legacy", "org_legacy"), "/api/v1/me")).body,
      ).toMatchObject({ displayName: "Legacy" });
      // Legacy pairs a device from their WorkOS sign-in; their own account is their sign-in already.
      const legacyKey = agentKey();
      const own = await ownInvite(await workos.sign("user_legacy", "org_legacy"));
      expect(own.status).toBe(201);
      const legacyDevice = await redeem(own.body.invite, legacyKey);
      expect((await call(legacyKey, "/api/v1/me")).body.displayName).toBe("Legacy");
      const listed = await call(legacyKey, "/api/v1/me/devices");
      expect(
        listed.body.items.find((item: { id: string }) => item.id === legacyDevice.credentialId),
      ).toMatchObject({ invitedByUserId: legacyDevice.memberId, invitedByDeviceId: null });
      const yours = await link(legacyKey, await workos.sign("user_legacy", "org_legacy"));
      expect(yours.status).toBe(409);
      expect(yours.body.detail).toContain("already your sign-in");
      expect(registered.body.detail).toContain("registered as a member already");

      // Another pairing organisation's owner cannot link the tenant this one links.
      const elsewhere = await bootstrap("elsewhere", "Other Owner");
      const key = agentKey();
      await redeem(elsewhere.invite.token, key);
      const refused = await link(key, await entraOf(crypto.randomUUID()));
      expect(refused.status).toBe(409);
      expect(refused.body.type).toBe(problemTypes.identityConflict);
    });

    test("members and administrators unlink accounts, which then sign in as no one", async () => {
      const person = await invitedPerson("Peder Angove");
      const personOid = crypto.randomUUID();
      const linked = await link(person.key, await entraOf(personOid));
      expect(linked.status).toBe(201);
      expect((await call(person.key, `/api/v1/members/${owner.id}/identities`)).status).toBe(403);
      const listed = await call(owner.key, `/api/v1/members/${person.id}/identities`);
      expect(listed.body.items.map((item: { id: string }) => item.id)).toEqual([linked.body.id]);
      const unlinked = await call(
        owner.key,
        `/api/v1/members/${person.id}/identities/${linked.body.id}`,
        "DELETE",
      );
      expect(unlinked.status).toBe(200);
      expect(await audits("memberIdentityUnlinked", linked.body.id)).toHaveLength(1);
      expect((await call(await entraOf(personOid), "/api/v1/me")).status).toBe(403);
      expect((await call(person.key, "/api/v1/me")).status).toBe(200);
      expect(
        (
          await call(
            owner.key,
            `/api/v1/members/${person.id}/identities/${linked.body.id}`,
            "DELETE",
          )
        ).status,
      ).toBe(404);

      const workosLink = (await call(owner.key, "/api/v1/me/identities")).body.items.find(
        (item: { provider: string }) => item.provider === "workos",
      );
      const mine = await call(owner.key, `/api/v1/me/identities/${workosLink.id}`, "DELETE");
      expect(mine.status).toBe(200);
      expect(
        (await call(await workos.sign("user_owner", "org_example"), "/api/v1/me")).status,
      ).toBe(403);
    });
  });

  describe("linking races and bound tenants", () => {
    const policy: SignInPolicy = {
      autoRegister: new Set(["entra"]),
      entraAdminRole: "Subspace.Admin",
      reservedUsernames: new Set(),
      registerGuests: false,
    };
    /**
     * Signs an Entra account in with `land` committed on another connection after sign-in's
     * first lookup and before registration's transaction: the window in which a link can win.
     */
    const signInAcross = (tenantId: string, subject: string, land: () => Promise<unknown>) => {
      let landed = false;
      const racing = Layer.effect(
        Database,
        Effect.gen(function* () {
          const db = yield* Database;
          const transaction: typeof db.transaction = (...args) =>
            Effect.andThen(
              Effect.promise(async () => {
                if (!landed) await land();
                landed = true;
              }),
              db.transaction(...args),
            );
          return new Proxy(db, {
            get: (target, property, receiver) =>
              property === "transaction" ? transaction : Reflect.get(target, property, receiver),
          });
        }),
      ).pipe(Layer.provide(databaseLayer(database.url, 2)));
      return Effect.runPromise(
        signedInMember(
          {
            provider: "entra",
            externalOrganisation: tenantId,
            subject,
            expiresAt: Math.floor(Date.now() / 1000) + 3600,
            claims: { roles: ["Subspace.Admin"], name: "Racing Person" },
          },
          policy,
        ).pipe(Effect.provide(racing)),
      );
    };
    const insertMember = async (organisationId: string, subject: string, username: string) =>
      (
        await database.client.query(
          "INSERT INTO users (organisation_id, subject, display_name, role, username) VALUES ($1, $2, $3, 'member', $3) RETURNING id",
          [organisationId, subject, username],
        )
      ).rows[0].id as string;
    const insertLink = (
      organisationId: string,
      userId: string,
      tenantId: string,
      subject: string,
    ) =>
      database.client.query(
        "INSERT INTO member_identities (organisation_id, user_id, provider, external_organisation, subject) VALUES ($1, $2, 'entra', $3, $4)",
        [organisationId, userId, tenantId, subject],
      );

    test("a link in the tenant's own organisation that wins the race signs in without the token's role", async () => {
      const tenantId = crypto.randomUUID();
      const [{ id: own }] = (
        await database.client.query(
          "INSERT INTO organisations (provider, external_id) VALUES ('entra', $1) RETURNING id",
          [tenantId],
        )
      ).rows;
      const member = await insertMember(own, crypto.randomUUID(), "kerensa");
      const subject = crypto.randomUUID();
      const signedIn = await signInAcross(tenantId, subject, () =>
        insertLink(own, member, tenantId, subject),
      );
      expect(signedIn).toMatchObject({ id: member, role: "member", tokenRole: null });
      const [row] = (
        await database.client.query("SELECT role, token_role FROM users WHERE id = $1", [member])
      ).rows;
      expect(row).toEqual({ role: "member", token_role: null });
      expect(await audits("memberRoleChanged", member)).toEqual([]);
      expect(
        (await database.client.query("SELECT id FROM users WHERE subject = $1", [subject])).rows,
      ).toEqual([]);
    });

    test("a link into another organisation that wins the race signs in, not a 403", async () => {
      const tenantId = crypto.randomUUID();
      const [{ organisation_id: elsewhere }] = (
        await database.client.query("SELECT organisation_id FROM users WHERE id = $1", [owner.id])
      ).rows;
      const member = await insertMember(elsewhere, `race-${crypto.randomUUID()}`, "morwenna");
      const subject = crypto.randomUUID();
      const signedIn = await signInAcross(tenantId, subject, () =>
        insertLink(elsewhere, member, tenantId, subject),
      );
      expect(signedIn).toMatchObject({ id: member, role: "member", tokenRole: null });
      expect(
        (
          await database.client.query(
            "SELECT id FROM organisations WHERE provider = 'entra' AND external_id = $1",
            [tenantId],
          )
        ).rows,
      ).toEqual([]);
    });

    test("the membership command adds no one to a bound tenant's own organisation", async () => {
      const tenantId = crypto.randomUUID();
      const [{ organisation_id: elsewhere }] = (
        await database.client.query("SELECT organisation_id FROM users WHERE id = $1", [owner.id])
      ).rows;
      const member = await insertMember(elsewhere, `bound-${crypto.randomUUID()}`, "jenifer");
      await insertLink(elsewhere, member, tenantId, crypto.randomUUID());
      const provisioned = await Effect.runPromiseExit(
        provisionMember(
          { provider: "entra", externalOrganisation: tenantId, subject: crypto.randomUUID() },
          { displayName: "Split Tenant", email: null },
          "member",
        ).pipe(Effect.provide(databaseLayer(database.url, 2))),
      );
      expect(Exit.isFailure(provisioned)).toBe(true);
      expect(String(Exit.isFailure(provisioned) && provisioned.cause)).toContain(
        "linked into another organisation",
      );
      expect(
        (
          await database.client.query(
            "SELECT id FROM organisations WHERE provider = 'entra' AND external_id = $1",
            [tenantId],
          )
        ).rows,
      ).toEqual([]);
    });
  });
});
