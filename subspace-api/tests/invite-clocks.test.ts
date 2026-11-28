import { afterAll, beforeAll, describe, expect, setSystemTime, test } from "bun:test";
import { agentKey } from "./certificates";
import { entraTenant, isolatedDatabase, startApi, testDatabase } from "./entra-stack";

/**
 * An invite's revocation and redemption are dated by the database's clock, as its creation and
 * the queries that judge it pending are, never by the API process's: so a replica whose clock is
 * behind cannot record either before the invite existed or before the device it paired.
 */

const deviceAudience = "https://subspace.example.internal";
const adminRole = "Subspace.Admin";
/** The API process's clock, two hours behind the database's. */
const behind = 2 * 3600_000;

describe.skipIf(!testDatabase)("invite times against a skewed process clock", () => {
  let database: Awaited<ReturnType<typeof isolatedDatabase>>;
  let tenant: Awaited<ReturnType<typeof entraTenant>>;
  let api: Awaited<ReturnType<typeof startApi>>;

  beforeAll(async () => {
    database = await isolatedDatabase("invite_clocks");
    tenant = await entraTenant();
    api = await startApi(
      database.url,
      {
        ...tenant.env,
        NODE_ENV: "production",
        AUTH_PROVIDERS: "entra,pairing",
        ENTRA_ADMIN_ROLE: adminRole,
        DEVICE_TOKEN_AUDIENCE: deviceAudience,
      },
      tenant.fetchRemote,
    );
  });
  afterAll(async () => {
    setSystemTime();
    await api?.close();
    await database?.drop();
  });

  /**
   * Runs `steps` with this process's clock behind the database's. The API runs in this process,
   * and the tokens it checks are signed here too, so they agree with it: only the database differs.
   */
  const skewed = async <T>(steps: () => Promise<T>) => {
    setSystemTime(new Date(Date.now() - behind));
    try {
      return await steps();
    } finally {
      setSystemTime();
    }
  };
  const call = async (token: string, path: string, method = "GET", body?: unknown) => {
    const response = await api.call(path, token, method, body);
    return { status: response.status, body: await response.json() };
  };
  /** An administrator by the app role, with a fresh token signed by this process's clock. */
  const admin = (oid: string, isAdmin = true) =>
    tenant.sign(oid, { roles: isAdmin ? [adminRole] : [], name: `Admin ${oid.slice(0, 6)}` });
  /** An invite for a new person, issued by the administrator `oid`. */
  const issue = async (oid: string, displayName: string) => {
    const issued = await call(await admin(oid), "/api/v1/pairing/invites", "POST", {
      displayName,
    });
    expect(issued.status).toBe(201);
    return issued.body as { inviteId: string; invite: string };
  };
  const times = async (inviteId: string) =>
    (
      await database.client.query(
        "SELECT created_at, revoked_at, redeemed_at FROM pairing_invites WHERE id = $1",
        [inviteId],
      )
    ).rows[0] as { created_at: Date; revoked_at: Date | null; redeemed_at: Date | null };

  test("an administrator's revocation of an invite is never dated before it", async () => {
    const oid = crypto.randomUUID();
    const { inviteId } = await issue(oid, "Tamsin Rowe");
    const revoked = await skewed(async () =>
      call(await admin(oid), `/api/v1/pairing/invites/${inviteId}`, "DELETE"),
    );
    expect(revoked.status).toBe(200);
    const row = await times(inviteId);
    expect(row.revoked_at).not.toBeNull();
    expect(row.revoked_at!.getTime()).toBeGreaterThanOrEqual(row.created_at.getTime());
  });

  test("revoking a demoted issuer's invites never dates them before they were issued", async () => {
    const oid = crypto.randomUUID();
    const { inviteId } = await issue(oid, "Piran Hosking");
    // Their next sign-in records that the token no longer makes them an administrator.
    const me = await skewed(async () => call(await admin(oid, false), "/api/v1/me"));
    expect(me.status).toBe(200);
    expect(me.body.isAdmin).toBe(false);
    const row = await times(inviteId);
    expect(row.revoked_at).not.toBeNull();
    expect(row.revoked_at!.getTime()).toBeGreaterThanOrEqual(row.created_at.getTime());
  });

  test("a redemption is never dated before the invite or the device it pairs", async () => {
    const oid = crypto.randomUUID();
    const { inviteId, invite } = await issue(oid, "Ysella Bray");
    const redeemed = await skewed(async () => {
      const response = await api.call("/api/v1/pairing/redeem", undefined, "POST", {
        invite,
        certificate: agentKey().certificate,
      });
      return { status: response.status, body: await response.json() };
    });
    expect(redeemed.status).toBe(201);
    const row = await times(inviteId);
    expect(row.redeemed_at).not.toBeNull();
    expect(row.redeemed_at!.getTime()).toBeGreaterThanOrEqual(row.created_at.getTime());
    const device = await database.client.query(
      "SELECT created_at FROM agent_credentials WHERE id = $1",
      [redeemed.body.credentialId],
    );
    expect(row.redeemed_at!.getTime()).toBeGreaterThanOrEqual(
      (device.rows[0].created_at as Date).getTime(),
    );
  });
});
