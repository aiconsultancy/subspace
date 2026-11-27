import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { Effect } from "effect";
import { Database, databaseLayer } from "../src/db";
import { provisionMember } from "../src/registration";
import { applyTokenRole } from "../src/registration";
import { isolatedDatabase, testDatabase } from "./entra-stack";

describe.skipIf(!testDatabase)("invites of a member who may no longer issue them", () => {
  let database: Awaited<ReturnType<typeof isolatedDatabase>>;
  const tenant = crypto.randomUUID();

  beforeAll(async () => {
    database = await isolatedDatabase("invite_issuers");
  });
  afterAll(async () => {
    await database?.drop();
  });

  const run = <A, E>(effect: Effect.Effect<A, E, Database>) =>
    Effect.runPromise(effect.pipe(Effect.provide(databaseLayer(database.url, 2))));
  const provision = (subject: string, role: "admin" | "member", active = true) =>
    run(
      provisionMember(
        { provider: "entra", externalOrganisation: tenant, subject },
        { displayName: `Person ${subject.slice(0, 8)}`, email: null },
        role,
        active,
      ),
    );
  const pending = async (organisationId: string, userId: string, createdById: string) =>
    (
      await database.client.query(
        `INSERT INTO pairing_invites (organisation_id, user_id, created_by_id, token_hash, expires_at)
         VALUES ($1, $2, $3, $4, now() + interval '1 hour') RETURNING id`,
        [organisationId, userId, createdById, Bun.SHA256.hash(crypto.randomUUID(), "hex")],
      )
    ).rows[0].id as string;
  const revoked = async (id: string) =>
    (await database.client.query("SELECT revoked_at FROM pairing_invites WHERE id = $1", [id]))
      .rows[0].revoked_at !== null;
  const audit = async (id: string) =>
    (
      await database.client.query(
        "SELECT \"actorType\" AS actor_type, actor_id, actor_display_name, source, changes FROM audit_logs WHERE action = 'pairingInviteRevoked' AND entity_id = $1",
        [id],
      )
    ).rows;

  test("the membership command revokes them when it takes the role away or deactivates", async () => {
    const admin = await provision(crypto.randomUUID(), "admin");
    const person = await provision(crypto.randomUUID(), "member");
    const forPerson = await pending(admin.organisationId, person.id, admin.id);
    const own = await pending(admin.organisationId, admin.id, admin.id);
    // Saving the administrator again as one changes nothing.
    await provision(admin.subject, "admin");
    expect(await revoked(forPerson)).toBe(false);

    await provision(admin.subject, "member");
    expect(await revoked(forPerson)).toBe(true);
    expect(await revoked(own)).toBe(false);
    expect(await audit(forPerson)).toEqual([
      {
        actor_type: "integration",
        actor_id: null,
        actor_display_name: "Membership command",
        source: "cli",
        changes: { memberId: person.id, issuerId: admin.id },
      },
    ]);

    // Deactivation does the same, even for an administrator.
    await provision(admin.subject, "admin");
    const again = await pending(admin.organisationId, person.id, admin.id);
    await provision(admin.subject, "admin", false);
    expect(await revoked(again)).toBe(true);
    expect(await audit(again)).toHaveLength(1);
  });

  test("an Entra token without the administrator role revokes them", async () => {
    const admin = await provision(crypto.randomUUID(), "member");
    const person = await provision(crypto.randomUUID(), "member");
    const promoted = await run(applyTokenRole(admin, "admin"));
    const forPerson = await pending(admin.organisationId, person.id, admin.id);
    const own = await pending(admin.organisationId, admin.id, admin.id);

    await run(applyTokenRole(promoted, "member"));
    expect(await revoked(forPerson)).toBe(true);
    expect(await revoked(own)).toBe(false);
    const [entry] = await audit(forPerson);
    expect(entry).toMatchObject({
      actor_type: "human",
      actor_id: admin.id,
      changes: { memberId: person.id, issuerId: admin.id },
    });
  });
});
