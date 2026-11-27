import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { Effect } from "effect";
import { databaseLayer } from "../src/db";
import { bootstrapOwner } from "../src/pairing-owner";
import { problemTypes } from "../src/problems";
import { agentKey, deviceToken, type AgentKey } from "./certificates";
import { entraTenant, isolatedDatabase, startApi, testDatabase } from "./entra-stack";

/**
 * Recovering a stolen workstation with the owner bootstrap command: `--revoke-devices` revokes
 * the owner's devices and everything they issued, as revoking them through the API does, so the
 * thief keeps no foothold through an invite or a device paired with one.
 */

const deviceAudience = "https://subspace.example.internal";

describe.skipIf(!testDatabase)("owner recovery with --revoke-devices", () => {
  let database: Awaited<ReturnType<typeof isolatedDatabase>>;
  let api: Awaited<ReturnType<typeof startApi>>;

  beforeAll(async () => {
    database = await isolatedDatabase("owner_recovery");
    const tenant = await entraTenant();
    api = await startApi(
      database.url,
      {
        ...tenant.env,
        NODE_ENV: "production",
        AUTH_PROVIDERS: "entra,pairing",
        DEVICE_TOKEN_AUDIENCE: deviceAudience,
      },
      tenant.fetchRemote,
    );
  });
  afterAll(async () => {
    await api?.close();
    await database?.drop();
  });

  const bootstrap = (revoke = false) =>
    Effect.runPromise(
      bootstrapOwner({
        externalOrganisation: "recovery",
        displayName: "Wenna Polglaze",
        email: null,
        lifetime: 3600,
        devices: revoke ? "revoke" : undefined,
        reserved: new Set(),
      }).pipe(Effect.provide(databaseLayer(database.url, 2))),
    );
  const redeem = (invite: string, key: AgentKey) =>
    api.call("/api/v1/pairing/redeem", undefined, "POST", { invite, certificate: key.certificate });
  /** A new device that redeems `invite`, and its credential's ID. */
  const pair = async (invite: string) => {
    const key = agentKey();
    const response = await redeem(invite, key);
    expect(response.status).toBe(201);
    return { key, credentialId: (await response.json()).credentialId as string };
  };
  const call = async (key: AgentKey, path: string, method = "GET", body?: unknown) => {
    const response = await api.call(path, await deviceToken(key, deviceAudience), method, body);
    return { status: response.status, body: await response.json() };
  };
  /** An invite that `issuer` issues for a new person, made an administrator if asked. */
  const inviteFrom = async (issuer: AgentKey, displayName: string, admin = false) => {
    const issued = await call(issuer, "/api/v1/pairing/invites", "POST", { displayName });
    expect(issued.status).toBe(201);
    if (admin)
      await database.client.query("UPDATE users SET role = 'admin' WHERE id = $1", [
        issued.body.memberId,
      ]);
    return issued.body as { invite: string; inviteId: string; memberId: string };
  };

  test("revokes the stolen device's pending invites and the devices paired from it, as the host", async () => {
    const first = await bootstrap();
    const stolen = await pair(first.invite.token);
    // From the stolen laptop: an invite still pending, and a branch two devices deep.
    const pending = await inviteFrom(stolen.key, "Morwenna Hext");
    const lieutenant = await pair((await inviteFrom(stolen.key, "Jenifer Rowse", true)).invite);
    const recruit = await pair((await inviteFrom(lieutenant.key, "Cadan Trewin")).invite);
    for (const device of [stolen, lieutenant, recruit])
      expect((await call(device.key, "/api/v1/me")).status).toBe(200);

    const recovery = await bootstrap(true);
    expect(recovery).toMatchObject({ revokedDevices: 1, cascade: { devices: 2, invites: 1 } });

    // Refused at once, every one of them.
    for (const device of [stolen, lieutenant, recruit])
      expect((await call(device.key, "/api/v1/me")).status).toBe(401);
    const refused = await redeem(pending.invite, agentKey());
    expect(refused.status).toBe(401);
    expect((await refused.json()).type).toBe(problemTypes.pairingInviteUnusable);

    // Each revocation is audited as the command on the API host, which no member is.
    const { rows } = await database.client.query(
      `SELECT action, entity_id, "actorType" AS actor_type, actor_id, source, changes FROM audit_logs WHERE entity_id = ANY($1) AND action IN ('deviceCredentialRevoked', 'pairingInviteRevoked') ORDER BY entity_id`,
      [[stolen.credentialId, lieutenant.credentialId, recruit.credentialId, pending.inviteId]],
    );
    expect(rows).toHaveLength(4);
    for (const row of rows)
      expect(row).toMatchObject({ actor_type: "integration", actor_id: null, source: "cli" });
    expect(rows.find((row) => row.entity_id === stolen.credentialId)?.changes).toMatchObject({
      cascade: { devices: 2, invites: 1 },
    });
    expect(rows.find((row) => row.entity_id === recruit.credentialId)?.changes).toMatchObject({
      issuingDeviceId: lieutenant.credentialId,
    });
    const { rows: revokers } = await database.client.query(
      "SELECT DISTINCT revoked_by_id FROM agent_credentials WHERE id = ANY($1)",
      [[stolen.credentialId, lieutenant.credentialId, recruit.credentialId]],
    );
    expect(revokers).toEqual([{ revoked_by_id: null }]);

    // The owner's new invite pairs their new laptop, which signs in and invites again.
    const laptop = await pair(recovery.invite.token);
    expect((await call(laptop.key, "/api/v1/me")).body.userId).toBe(recovery.owner.id);
    expect((await inviteFrom(laptop.key, "Morwenna Hext")).invite).toBeDefined();
  });
});
