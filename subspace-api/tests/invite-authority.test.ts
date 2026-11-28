import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { readFile } from "node:fs/promises";
import { Effect } from "effect";
import { databaseLayer } from "../src/db";
import { bootstrapOwner } from "../src/pairing-owner";
import { problemTypes } from "../src/problems";
import { agentKey, deviceToken, type AgentKey } from "./certificates";
import pg from "pg";
import { entraTenant, isolatedDatabase, lockWaiters, startApi, testDatabase } from "./entra-stack";

/**
 * An invite for another person works only while its issuer still vouches for it: still an
 * administrator from the source their role came from when they issued it, under the deployment's
 * configuration as it is at redemption, and with the device that issued it unrevoked.
 */

const deviceAudience = "https://subspace.example.internal";
const adminRole = "Subspace.Admin";

describe.skipIf(!testDatabase)("invites against their issuer's authority", () => {
  let database: Awaited<ReturnType<typeof isolatedDatabase>>;
  let tenant: Awaited<ReturnType<typeof entraTenant>>;
  /** The API with ENTRA_ADMIN_ROLE, and the same deployment once the setting is removed. */
  let mapped: Awaited<ReturnType<typeof startApi>>;
  let unmapped: Awaited<ReturnType<typeof startApi>>;

  beforeAll(async () => {
    database = await isolatedDatabase("invite_authority");
    tenant = await entraTenant();
    const env = {
      ...tenant.env,
      NODE_ENV: "production",
      AUTH_PROVIDERS: "entra,pairing",
      DEVICE_TOKEN_AUDIENCE: deviceAudience,
    };
    mapped = await startApi(
      database.url,
      { ...env, ENTRA_ADMIN_ROLE: adminRole },
      tenant.fetchRemote,
    );
    unmapped = await startApi(database.url, env, tenant.fetchRemote);
  });
  afterAll(async () => {
    await mapped?.close();
    await unmapped?.close();
    await database?.drop();
  });

  type Api = typeof mapped;
  const call = async (api: Api, token: AgentKey | string, path: string, body?: unknown) => {
    const bearer = typeof token === "string" ? token : await deviceToken(token, deviceAudience);
    const response = await api.call(path, bearer, body === undefined ? "GET" : "POST", body);
    return { status: response.status, body: await response.json() };
  };
  const redeem = async (api: Api, invite: string) => {
    const response = await api.call("/api/v1/pairing/redeem", undefined, "POST", {
      invite,
      certificate: agentKey().certificate,
    });
    return { status: response.status, body: await response.json() };
  };
  const refused = (redeemed: { status: number; body: { type?: string } }) => {
    expect(redeemed.status).toBe(401);
    expect(redeemed.body.type).toBe(problemTypes.pairingInviteUnusable);
  };
  /** An invite for a new person, issued by `issuer` through `api`. */
  const invite = async (api: Api, issuer: AgentKey | string, displayName: string) => {
    const issued = await call(api, issuer, "/api/v1/pairing/invites", { displayName });
    expect(issued.status).toBe(201);
    return issued.body.invite as string;
  };
  /** A member of the tenant, an administrator by the app role alone, signed in once. */
  const entraAdmin = async () => {
    const oid = crypto.randomUUID();
    const token = await tenant.sign(oid, { roles: [adminRole], name: `Admin ${oid.slice(0, 6)}` });
    const me = await call(mapped, token, "/api/v1/me");
    expect(me.body.isAdmin).toBe(true);
    return { oid, token, id: me.body.userId as string };
  };
  /** A device of `oid` paired with their own invite, issued through `api`. */
  const pairOwn = async (token: string) => {
    const issued = await call(mapped, token, "/api/v1/me/pairing/invites", {});
    expect(issued.status).toBe(201);
    const key = agentKey();
    const response = await mapped.call("/api/v1/pairing/redeem", undefined, "POST", {
      invite: issued.body.invite,
      certificate: key.certificate,
    });
    expect(response.status).toBe(201);
    return { key, credentialId: (await response.json()).credentialId as string };
  };

  test("removing ENTRA_ADMIN_ROLE withdraws the invites its role issued, with no one signing in", async () => {
    const admin = await entraAdmin();
    const first = await invite(mapped, admin.token, "Sennen Vosper");
    const second = await invite(mapped, admin.token, "Bryok Angwin");
    // A provisioned administrator's invite, from their device, does not depend on the setting.
    const provisioned = await entraAdmin();
    await database.client.query("UPDATE users SET role = 'admin' WHERE id = $1", [provisioned.id]);
    const { key } = await pairOwn(provisioned.token);
    const theirs = await invite(mapped, key, "Tegen Rosewarne");

    // Without the setting, the app role makes no one an administrator, and no invite it issued
    // pairs a device; the provisioned administrator's still does.
    refused(await redeem(unmapped, first));
    expect((await redeem(unmapped, theirs)).status).toBe(201);
    // With it, the same issuer's other invite still pairs: the check is the source, not the row.
    expect((await redeem(mapped, second)).status).toBe(201);
  });

  test("the pending list leaves out the invites redemption would refuse for their issuer", async () => {
    const issue = async (api: Api, issuer: AgentKey | string, displayName: string) => {
      const issued = await call(api, issuer, "/api/v1/pairing/invites", { displayName });
      expect(issued.status).toBe(201);
      return issued.body.inviteId as string;
    };
    const listed = async (api: Api, token: AgentKey | string) => {
      const list = await call(api, token, "/api/v1/pairing/invites");
      expect(list.status).toBe(200);
      return new Set((list.body.items as { id: string }[]).map((item) => item.id));
    };
    const appRoleOnly = await entraAdmin();
    const byAppRole = await issue(mapped, appRoleOnly.token, "Demelza Trevail");
    const provisioned = await entraAdmin();
    await database.client.query("UPDATE users SET role = 'admin' WHERE id = $1", [provisioned.id]);
    const device = await pairOwn(provisioned.token);
    const byDevice = await issue(mapped, device.key, "Cador Nankervis");
    const byToken = await issue(mapped, provisioned.token, "Elowen Pascoe");
    for (const id of [byAppRole, byDevice, byToken])
      expect((await listed(mapped, device.key)).has(id)).toBe(true);

    // Without ENTRA_ADMIN_ROLE, the invite only the app role vouched for is gone.
    const unmappedList = await listed(unmapped, device.key);
    expect(unmappedList.has(byAppRole)).toBe(false);
    expect(unmappedList.has(byDevice)).toBe(true);
    expect(unmappedList.has(byToken)).toBe(true);
    // A device revoked outside the walk, as the part-one image does: its invite is gone too.
    await database.client.query("UPDATE agent_credentials SET revoked_at = now() WHERE id = $1", [
      device.credentialId,
    ]);
    const mappedList = await listed(mapped, provisioned.token);
    expect(mappedList.has(byDevice)).toBe(false);
    expect(mappedList.has(byAppRole)).toBe(true);
    expect(mappedList.has(byToken)).toBe(true);
  });

  test("an invite records no issuer authority but the two redemption knows", async () => {
    const admin = await entraAdmin();
    const issued = await call(mapped, admin.token, "/api/v1/pairing/invites", {
      displayName: "Gwen Treloar",
    });
    expect(issued.status).toBe(201);
    const set = (value: string | null) =>
      database.client.query("UPDATE pairing_invites SET issuer_authority = $2 WHERE id = $1", [
        issued.body.inviteId,
        value,
      ]);
    for (const value of ["tokenRole", "admin", "", "Provisioned"])
      await expect(set(value)).rejects.toMatchObject({
        code: "23514",
        constraint: "pairing_invites_issuer_authority",
      });
    for (const value of ["provisioned", "entraAppRole", null]) await set(value);
  });

  test("an administrator by both roles keeps their invites without ENTRA_ADMIN_ROLE, never above their recorded token role", async () => {
    const admin = await entraAdmin();
    await database.client.query("UPDATE users SET role = 'admin' WHERE id = $1", [admin.id]);
    /** The authority an invite records; its text begins with its 16-byte ID. */
    const authority = async (invite: string) => {
      const id = Buffer.from(invite.slice("spair1.".length), "base64url")
        .subarray(0, 16)
        .toString("hex");
      const { rows } = await database.client.query(
        "SELECT issuer_authority FROM pairing_invites WHERE id = $1::uuid",
        [id],
      );
      return rows[0].issuer_authority as string;
    };
    // Issued with their own Entra token, whose app role makes them an administrator as their
    // provisioned role does: the invite records the provisioned role, which outlives the setting.
    const kept = await invite(mapped, admin.token, "Lowena Hocking");
    expect(await authority(kept)).toBe("provisioned");
    expect((await redeem(unmapped, kept)).status).toBe(201);
    // While the setting is configured, the provisioned role is never above the recorded token role:
    // once an Entra sign-in records that the token no longer makes them an administrator, no
    // invite of theirs pairs, as their device would no longer act as an administrator.
    const capped = await invite(mapped, admin.token, "Jory Penhallow");
    await database.client.query("UPDATE users SET token_role = 'member' WHERE id = $1", [admin.id]);
    refused(await redeem(mapped, capped));
    // An administrator by the app role alone records it.
    const appRoleOnly = await entraAdmin();
    expect(await authority(await invite(mapped, appRoleOnly.token, "Morwenna Udy"))).toBe(
      "entraAppRole",
    );
  });

  test("an issuer's sign-in that withdraws their role waits for a redemption of their invite, rather than deadlocking with it", async () => {
    const admin = await entraAdmin();
    const issued = await call(mapped, admin.token, "/api/v1/pairing/invites", {
      displayName: "Kerensa Tregear",
    });
    expect(issued.status).toBe(201);
    const { rows } = await database.client.query(
      "SELECT organisation_id FROM users WHERE id = $1",
      [admin.id],
    );
    // A redemption, part way: the organisation's row and the invite locked, its issuer not yet read.
    const redemption = new pg.Client({ connectionString: database.url });
    await redemption.connect();
    let signedIn: Promise<{ status: number; body: { isAdmin?: boolean } }> | undefined;
    try {
      await redemption.query("BEGIN");
      await redemption.query("SET LOCAL lock_timeout = '4s'");
      await redemption.query("SELECT id FROM organisations WHERE id = $1 FOR NO KEY UPDATE", [
        rows[0].organisation_id,
      ]);
      await redemption.query("SELECT id FROM pairing_invites WHERE id = $1 FOR UPDATE", [
        issued.body.inviteId,
      ]);
      // The issuer signs in with a token that no longer carries the app role.
      const withdrawn = await tenant.sign(admin.oid, { roles: [], name: "Withdrawn admin" });
      signedIn = call(mapped, withdrawn, "/api/v1/me");
      signedIn.catch(() => {});
      await lockWaiters(redemption, 1);
      // The redemption reads its issuer, as `issuerRefusal` does: the sign-in holds nothing yet.
      await redemption.query("SELECT id FROM users WHERE id = $1 FOR SHARE", [admin.id]);
      await redemption.query("COMMIT");
    } finally {
      await redemption.query("ROLLBACK").catch(() => {});
      await redemption.end();
    }
    const me = await signedIn!;
    expect(me.status).toBe(200);
    expect(me.body.isAdmin).toBe(false);
    const invite = await database.client.query(
      "SELECT revoked_at FROM pairing_invites WHERE id = $1",
      [issued.body.inviteId],
    );
    expect(invite.rows[0].revoked_at).not.toBeNull();
  });

  test("an invite is refused once its issuing device is revoked without its invites", async () => {
    const admin = await entraAdmin();
    await database.client.query("UPDATE users SET role = 'admin' WHERE id = $1", [admin.id]);
    const device = await pairOwn(admin.token);
    const forOther = await invite(mapped, device.key, "Gwithian Penrose");
    const own = await call(mapped, device.key, "/api/v1/me/pairing/invites", {});
    // As the part-one image, or a direct update, revokes a device: its invites stay pending.
    await database.client.query("UPDATE agent_credentials SET revoked_at = now() WHERE id = $1", [
      device.credentialId,
    ]);
    refused(await redeem(mapped, forOther));
    refused(await redeem(mapped, own.body.invite));
  });

  test("rolling back to part one withdraws part two's invites and leaves the owner's", async () => {
    const owner = await Effect.runPromise(
      bootstrapOwner({
        externalOrganisation: "rollback",
        displayName: "Ysella Couch",
        email: null,
        lifetime: 3600,
        reserved: new Set(),
      }).pipe(Effect.provide(databaseLayer(database.url, 2))),
    );
    const ownerKey = agentKey();
    expect(
      (
        await mapped.call("/api/v1/pairing/redeem", undefined, "POST", {
          invite: owner.invite.token,
          certificate: ownerKey.certificate,
        })
      ).status,
    ).toBe(201);
    const outstanding = await invite(mapped, ownerKey, "Pasco Jelbart");
    const own = (await call(mapped, ownerKey, "/api/v1/me/pairing/invites", {})).body
      .invite as string;
    const bootstrapped = await Effect.runPromise(
      bootstrapOwner({
        externalOrganisation: "rollback",
        lifetime: 3600,
        reserved: new Set(),
      }).pipe(Effect.provide(databaseLayer(database.url, 2))),
    );

    // The statement the rollback instructions give, run as they say, before part one starts.
    const guide = await readFile(
      new URL("../docs/changes/workstation-pairing/README.md", import.meta.url),
      "utf8",
    );
    const statement = guide.match(
      /```sql\n\s*(UPDATE pairing_invites SET revoked_at[^`]*)```/,
    )?.[1];
    expect(statement).toBeDefined();
    await database.client.query(statement!);

    // Part one redeems an invite that is not revoked, spent or expired: only the owner's is left.
    const pending = await database.client.query(
      "SELECT id FROM pairing_invites WHERE redeemed_at IS NULL AND revoked_at IS NULL AND expires_at > now()",
    );
    expect(pending.rows).toEqual([{ id: bootstrapped.invite.id }]);
    refused(await redeem(mapped, outstanding));
    refused(await redeem(mapped, own));
  });
});
