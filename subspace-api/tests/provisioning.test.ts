import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { eq } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { Database, databaseLayer } from "../src/db";
import { bootstrapOwner } from "../src/pairing-owner";
import { assignProject, createProject } from "../src/projects";
import { FleetProvisionSpec, provisionFleet, provisioningConfiguration } from "../src/provisioning";
import { projectRepositories, users } from "../src/schema";
import { agentKey, agentToken, type AgentKey } from "./certificates";
import { isolatedDatabase, startApi, testDatabase } from "./entra-stack";

const audience = "https://api.example.com";
const reserved = new Set<string>();

function spec(organisation: string, slug = "example") {
  return {
    organisation,
    owner: { name: "Aubrey Example", email: "aubrey@example.com" },
    fleet: {
      slug,
      name: `Fleet ${slug}`,
      ircHost: "irc.example.com",
      ircPort: 6697,
      ircTls: true,
      starbaseChannel: `#${slug}-starbase`,
      commandChannel: `#${slug}-command`,
    },
    rearAdmiral: { name: `${slug}-atlas`, ircNick: `${slug}-atlas`, model: "example-model" },
    captains: [
      {
        name: `${slug}-vega`,
        ircNick: `${slug}-vega`,
        model: "example-model",
        ship: "Vega",
        channel: `#${slug}-vega`,
      },
    ],
  } satisfies typeof FleetProvisionSpec.Type;
}

test.skipIf(process.getuid?.() === 0)(
  "the provisioning command refuses a non-root caller before reading input or configuration",
  () => {
    const command = Bun.spawnSync({
      cmd: [process.execPath, "scripts/provision-fleet.ts", "--file", "does-not-exist.json"],
      cwd: new URL("..", import.meta.url).pathname,
      env: {},
    });
    expect(command.exitCode).toBe(1);
    expect(command.stdout.byteLength).toBe(0);
    expect(command.stderr.toString()).toBe("Run this command as root on the API host.\n");
  },
);

test("the command's configuration errors name the setting, never its value", () => {
  const env = {
    AUTH_PROVIDERS: "pairing",
    AGENT_TOKEN_AUDIENCE: "https://api.example.com",
    DATABASE_URL: "postgresql://user:not-printed@db.example.com/subspace",
  };
  expect(provisioningConfiguration(env).url).toBe(env.DATABASE_URL);
  for (const [change, setting] of [
    [{ AUTH_PROVIDERS: "workos" }, "AUTH_PROVIDERS"],
    [{ AGENT_TOKEN_AUDIENCE: "" }, "AGENT_TOKEN_AUDIENCE"],
    // The API's own rule: plain http only on loopback, for the device audience too.
    [{ DEVICE_TOKEN_AUDIENCE: "http://api.example.com" }, "DEVICE_TOKEN_AUDIENCE"],
    [{ DATABASE_URL: "" }, "DATABASE_URL"],
  ] as const) {
    let message = "";
    try {
      provisioningConfiguration({ ...env, ...change });
    } catch (error) {
      message = (error as Error).message;
    }
    expect(message).toStartWith("The API configuration is not usable for provisioning: ");
    expect(message).toContain(setting);
    expect(message).not.toContain("not-printed");
  }
});

test("the specification does not accept key material or account subjects", () => {
  const input = spec("example");
  const decode = Schema.decodeUnknownSync(FleetProvisionSpec, { onExcessProperty: "error" });
  for (const field of ["certificate", "serviceTokenClientId", "secret"])
    expect(() =>
      decode({ ...input, rearAdmiral: { ...input.rearAdmiral, [field]: "not accepted" } }),
    ).toThrow();
});

test("the specification accepts only pairing organisation IDs", () => {
  const decode = Schema.decodeUnknownSync(FleetProvisionSpec);
  for (const id of ["a", "example-fleet.2", "x".repeat(63)])
    expect(decode(spec(id)).organisation).toBe(id);
  for (const id of ["", "pairing:example", "Example", "-example", "example\n", "x".repeat(64)])
    expect(() => decode(spec(id))).toThrow();
});

describe.skipIf(!testDatabase)("fleet provisioning on PostgreSQL", () => {
  let database: Awaited<ReturnType<typeof isolatedDatabase>>;
  let api: Awaited<ReturnType<typeof startApi>>;

  beforeAll(async () => {
    database = await isolatedDatabase("provision");
    api = await startApi(database.url, {
      NODE_ENV: "production",
      AUTH_PROVIDERS: "pairing",
      AGENT_TOKEN_AUDIENCE: audience,
    });
  });
  afterAll(async () => {
    await api?.close();
    await database?.drop();
  });

  const run = <A, E>(effect: Effect.Effect<A, E, Database>) =>
    Effect.runPromise(effect.pipe(Effect.provide(databaseLayer(database.url))));
  const provision = (input: typeof FleetProvisionSpec.Type) => run(provisionFleet(input, reserved));
  function enrol(token: string, name: string, key: AgentKey, ship?: string) {
    return api.call("/api/v1/agents/enrol", token, "POST", {
      name,
      rank: ship ? "captain" : "rearAdmiral",
      model: "example-model",
      ircNick: name,
      ship,
      certificate: key.certificate,
      scopes: ["api", "irc"],
    });
  }

  test("reconciles a fresh fleet and reuses all records and keys on rerun, without minting an invite", async () => {
    const input = spec("fresh");
    const start = Date.now();
    const first = await provision(input);
    const { grant } = first;
    expect(Date.parse(grant.expiresAt) - start).toBeGreaterThanOrEqual(600_000);
    expect(Date.parse(grant.expiresAt) - Date.now()).toBeLessThanOrEqual(600_000);
    expect(grant.maxUses).toBe(2);
    expect(first.ownerCreated).toBe(true);
    expect(first.projects).toEqual([]);
    expect(
      (
        await database.client.query(
          "SELECT provider, external_id FROM organisations WHERE id = $1",
          [first.organisationId],
        )
      ).rows,
    ).toEqual([{ provider: "pairing", external_id: "fresh" }]);
    expect(
      (
        await database.client.query(
          "SELECT starbase_channel, command_channel FROM fleets WHERE id = $1",
          [first.fleetId],
        )
      ).rows,
    ).toEqual([{ starbase_channel: "#example-starbase", command_channel: "#example-command" }]);
    expect(
      (
        await database.client.query("SELECT name, channel FROM ships WHERE fleet_id = $1", [
          first.fleetId,
        ])
      ).rows,
    ).toEqual([{ name: "Vega", channel: "#example-vega" }]);

    const [rearKey, captainKey, extraKey] = await Promise.all([agentKey(), agentKey(), agentKey()]);
    const [rear, captain] = [input.rearAdmiral, input.captains[0]!];
    const register = async (officer: string, key: AgentKey, ship?: string) => {
      const result = await enrol(grant.token, officer, key, ship);
      expect(result.status).toBe(201);
      return (await result.json()) as { agentId: string; credentialId: string };
    };
    const rearRegistration = await register(rear.name, rearKey);
    const captainRegistration = await register(captain.name, captainKey, captain.ship);
    expect(first.officers).toEqual([
      { name: rear.name, id: rearRegistration.agentId },
      { name: captain.name, id: captainRegistration.agentId },
    ]);
    expect((await enrol(grant.token, rear.name, extraKey)).status).toBe(401);

    const captainProfile = async () =>
      (
        await database.client.query(
          `SELECT subject, version, updated_at, agent_rank, agent_fleet_id, registered_by_id,
                  agent_model, agent_ship, agent_ship_id, agent_irc_nick, username
           FROM users WHERE id = $1`,
          [captainRegistration.agentId],
        )
      ).rows[0];
    const originalProfile = await captainProfile();
    const rerun = await provision({ ...input, owner: undefined });
    expect(rerun.ownerCreated).toBe(false);
    expect({ ...rerun, grant: undefined }).toEqual({
      ...first,
      ownerCreated: false,
      grant: undefined,
    });
    expect(await captainProfile()).toEqual(originalProfile);
    expect(rerun.grant.id).not.toBe(grant.id);
    const replay = await enrol(rerun.grant.token, rear.name, rearKey);
    expect(replay.status).toBe(200);
    expect(await replay.json()).toMatchObject(rearRegistration);
    const keys = await database.client.query(
      "SELECT id, revoked_at FROM agent_credentials WHERE organisation_id = $1 ORDER BY id",
      [first.organisationId],
    );
    expect(keys.rows).toEqual(
      [rearRegistration.credentialId, captainRegistration.credentialId]
        .sort()
        .map((id) => ({ id, revoked_at: null })),
    );
    expect((await api.call("/api/v1/me", await agentToken(rearKey, audience))).status).toBe(200);

    const audit = (
      await database.client.query(
        `SELECT action, source, "actorType" AS actor_type, actor_id, actor_display_name, changes
       FROM audit_logs WHERE organisation_id = $1 AND source = 'cli'`,
        [first.organisationId],
      )
    ).rows;
    expect(audit.map((event) => event.action).sort()).toEqual([
      "agentRegistered",
      "agentRegistered",
      "enrolmentTokenCreated",
      "enrolmentTokenCreated",
      "fleetCreated",
      "fleetInitiativeCreated",
      "memberRegistered",
      "shipCreated",
    ]);
    expect(
      audit.every(
        (event) =>
          event.actor_type === "integration" &&
          event.actor_id === null &&
          event.actor_display_name === "Fleet provisioning",
      ),
    ).toBe(true);
    expect(audit.some((event) => JSON.stringify(event).includes("senr1."))).toBe(false);
    expect(
      (
        await database.client.query("SELECT id FROM pairing_invites WHERE organisation_id = $1", [
          first.organisationId,
        ])
      ).rowCount,
    ).toBe(0);
    const uses = (
      await database.client.query(
        "SELECT id, uses FROM enrolment_tokens WHERE organisation_id = $1",
        [first.organisationId],
      )
    ).rows;
    expect(uses).toEqual(
      expect.arrayContaining([
        { id: grant.id, uses: 2 },
        { id: rerun.grant.id, uses: 0 },
      ]),
    );

    await database.client.query("UPDATE enrolment_tokens SET expires_at = now() WHERE id = $1", [
      rerun.grant.id,
    ]);
    const expired = await enrol(rerun.grant.token, rear.name, extraKey);
    expect(expired.status).toBe(401);
    expect(await expired.json()).toMatchObject({ detail: "This enrolment token has expired." });

    const changed = await provision({
      ...input,
      fleet: { ...input.fleet, ircHost: "new-irc.example.com" },
      captains: [{ ...captain, ircNick: "new-vega", model: "new-example-model", ship: "Altair" }],
      initiative: { id: first.initiativeId },
    });
    expect(changed.fleetId).toBe(first.fleetId);
    expect(changed.officers).toEqual(first.officers);
    expect(
      (await database.client.query("SELECT irc_host FROM fleets WHERE id = $1", [first.fleetId]))
        .rows,
    ).toEqual([{ irc_host: "new-irc.example.com" }]);
    const movedShip = (
      await database.client.query("SELECT id FROM ships WHERE fleet_id = $1 AND name = 'Altair'", [
        first.fleetId,
      ])
    ).rows[0];
    const updatedProfile = {
      ...originalProfile,
      version: originalProfile.version + 1,
      updated_at: expect.any(Date),
      agent_model: "new-example-model",
      agent_ship: "Altair",
      agent_ship_id: movedShip.id,
      agent_irc_nick: "new-vega",
      username: "new-vega",
    };
    expect(await captainProfile()).toEqual(updatedProfile);
    const captainReplay = await enrol(changed.grant.token, captain.name, captainKey, "Altair");
    expect(captainReplay.status).toBe(200);
    expect(await captainReplay.json()).toMatchObject(captainRegistration);
    expect(await captainProfile()).toEqual(updatedProfile);
  });

  test("leaves reactivation and an existing owner invite to the owner command", async () => {
    const input = spec("inactive-owner");
    const existing = await run(
      bootstrapOwner({
        externalOrganisation: input.organisation,
        displayName: input.owner.name,
        lifetime: 600,
        reserved,
      }),
    );
    const { client } = database;
    await client.query("UPDATE users SET active = false WHERE id = $1", [existing.owner.id]);

    await expect(provision(input)).rejects.toThrow("Use auth:owner with --reactivate");

    const { rows } = await client.query(
      `SELECT users.active, pairing_invites.revoked_at, audit_logs.actor_display_name
       FROM users JOIN pairing_invites ON pairing_invites.user_id = users.id
       JOIN audit_logs ON audit_logs.organisation_id = users.organisation_id
        AND audit_logs.entity_id = users.id::text AND audit_logs.action = 'memberRegistered'
       WHERE users.id = $1 AND pairing_invites.id = $2`,
      [existing.owner.id, existing.invite.id],
    );
    expect(rows).toEqual([
      { active: false, revoked_at: null, actor_display_name: "Owner bootstrap" },
    ]);
    const grants = await client.query(
      "SELECT id FROM enrolment_tokens WHERE organisation_id = $1",
      [existing.organisationId],
    );
    expect(grants.rows).toEqual([]);
  });

  test("refuses to move, rerank or reactivate a conflicting officer", async () => {
    const input = spec("officer-conflicts", "first");
    const first = await provision(input);
    const second = spec(input.organisation, "second");
    const captain = input.captains[0]!;
    const expectRefusal = (request: typeof FleetProvisionSpec.Type, detail: string) =>
      expect(provision(request)).rejects.toMatchObject({
        status: 409,
        detail: expect.stringContaining(detail),
      });
    await expectRefusal({ ...second, captains: input.captains }, "of fleet first has it");
    await expectRefusal(
      {
        ...input,
        rearAdmiral: { name: captain.name, ircNick: captain.ircNick, model: captain.model },
        captains: [],
      },
      "registered with the rank captain",
    );
    await database.client.query("UPDATE users SET active = false WHERE id = $1", [
      first.officers[1]!.id,
    ]);
    await expectRefusal(input, "is deactivated");
    const fleets = await database.client.query(
      "SELECT slug FROM fleets WHERE organisation_id = $1",
      [first.organisationId],
    );
    expect(fleets.rows).toEqual([{ slug: "first" }]);
    const officers = await database.client.query(
      "SELECT id, version, active FROM users WHERE organisation_id = $1 AND agent_rank IS NOT NULL ORDER BY display_name",
      [first.organisationId],
    );
    expect(officers.rows).toEqual([
      { id: first.officers[0]!.id, version: 1, active: true },
      { id: first.officers[1]!.id, version: 1, active: false },
    ]);
  });

  test("moves an existing initiative only on explicit adoption and reports project ownership without editing projects", async () => {
    const first = await provision(spec("adoption", "first"));
    const project = await run(
      Effect.gen(function* () {
        const db = yield* Database;
        const [owner] = yield* db.select().from(users).where(eq(users.id, first.ownerId));
        const created = yield* createProject(owner!, { key: "EX", name: "Example project" });
        const assigned = yield* assignProject(owner!, created.id, {
          version: created.version,
          initiativeId: first.initiativeId,
        });
        yield* db.insert(projectRepositories).values({
          organisationId: first.organisationId,
          projectId: created.id,
          repository: "example/repository",
        });
        return assigned;
      }),
    );
    const input = {
      ...spec("adoption", "second"),
      repositories: ["Example/Repository"],
      initiative: { id: first.initiativeId },
    };
    await expect(provision(input)).rejects.toThrow("Explicitly adopt");
    expect(
      (
        await database.client.query("SELECT slug FROM fleets WHERE organisation_id = $1", [
          first.organisationId,
        ])
      ).rows,
    ).toEqual([{ slug: "first" }]);
    expect(
      (
        await database.client.query("SELECT id FROM enrolment_tokens WHERE organisation_id = $1", [
          first.organisationId,
        ])
      ).rowCount,
    ).toBe(1);

    const second = await provision({ ...input, initiative: { ...input.initiative, adopt: true } });
    expect(second.initiativeId).toBe(first.initiativeId);
    expect(second.projects).toEqual([
      { id: project.id, key: "EX", repository: "example/repository", fleetId: second.fleetId },
    ]);
    expect(
      (
        await database.client.query(
          "SELECT id, initiative_id, version FROM projects WHERE organisation_id = $1",
          [first.organisationId],
        )
      ).rows,
    ).toEqual([{ id: project.id, initiative_id: first.initiativeId, version: project.version }]);
    expect(
      (
        await database.client.query(
          "SELECT changes FROM audit_logs WHERE organisation_id = $1 AND action = 'initiativeFleetAssigned'",
          [first.organisationId],
        )
      ).rows,
    ).toEqual([{ changes: { previousFleetId: first.fleetId, fleetId: second.fleetId } }]);
  });

  test("prefers exact-name initiatives on its own fleet and ignores archived branches", async () => {
    const input = spec("initiative-placement");
    const first = await provision(input);
    const { client } = database;
    const add = async (name: string, parentId: string | null = null, archived = false) =>
      (
        await client.query<{ id: string }>(
          `INSERT INTO initiatives (organisation_id, owner_id, name, parent_id, archived)
         VALUES ($1, $2, $3, $4, $5) RETURNING id`,
          [first.organisationId, first.ownerId, name, parentId, archived],
        )
      ).rows[0]!.id;
    await client.query("UPDATE initiatives SET name = 'Fleet root' WHERE id = $1", [
      first.initiativeId,
    ]);
    const target = await add(input.fleet.name, first.initiativeId);
    await add(input.fleet.name);
    await add(input.fleet.name.toUpperCase(), first.initiativeId);
    const archived = await add("Archived parent", first.initiativeId, true);
    const archivedChild = await add(input.fleet.name, archived);

    expect((await provision(input)).initiativeId).toBe(target);
    await expect(provision({ ...input, initiative: { id: archivedChild } })).rejects.toThrow(
      "Unarchive the initiative branch",
    );
    await add(input.fleet.name, first.initiativeId);
    await expect(provision(input)).rejects.toThrow("Several initiatives match");
  });

  test("a renamed fleet keeps its own initiative instead of gaining a second one", async () => {
    const input = spec("initiative-rename", "rename");
    const first = await provision(input);
    const renamed = { ...input, fleet: { ...input.fleet, name: "Renamed fleet" } };
    expect((await provision(renamed)).initiativeId).toBe(first.initiativeId);
    const count = async () =>
      (
        await database.client.query(
          "SELECT count(*)::int AS n FROM initiatives WHERE organisation_id = $1",
          [first.organisationId],
        )
      ).rows[0].n;
    expect(await count()).toBe(1);
    // Two initiatives of its own, neither with the fleet's name: it asks which one.
    await database.client.query(
      "INSERT INTO initiatives (organisation_id, owner_id, name, fleet_id) VALUES ($1, $2, 'Other work', $3)",
      [first.organisationId, first.ownerId, first.fleetId],
    );
    await expect(
      provision({ ...renamed, fleet: { ...renamed.fleet, name: "Renamed again" } }),
    ).rejects.toThrow("Several initiatives belong to this fleet");
    expect(await count()).toBe(2);
  });
});
