import { expect, test } from "bun:test";
import { mkdtemp, readdir, rm, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { migrate } from "drizzle-orm/effect-postgres/migrator";
import { Effect } from "effect";
import pg from "pg";
import { Database, databaseLayer } from "../src/db";
import { localDatabase } from "./local-database";

const testDatabase = process.env.TEST_DATABASE_URL;
const folder = new URL("../drizzle", import.meta.url).pathname;

function applyMigrations(url: string, migrationsFolder: string) {
  return Effect.runPromise(
    Effect.gen(function* () {
      const db = yield* Database;
      yield* migrate(db, { migrationsFolder });
    }).pipe(Effect.provide(databaseLayer(url))),
  );
}

test.skipIf(!testDatabase)(
  "fleet backfill links existing fleets and preserves unmatched agents on rerun",
  async () => {
    const local = localDatabase(testDatabase!);
    const databaseName = `fleet_migration_${crypto.randomUUID().replaceAll("-", "")}`;
    const admin = new pg.Client(local.settings());
    await admin.connect();
    try {
      await admin.query(`CREATE DATABASE "${databaseName}"`);
      const url = local.urlFor(databaseName);
      const previous = await mkdtemp(join(tmpdir(), "subspace-drizzle-"));
      const directories = (await readdir(folder)).sort();
      const fleetMigration = directories.find((name) => name.endsWith("_agent_fleet_ship_links"))!;
      try {
        for (const name of directories.filter((name) => name < fleetMigration))
          await symlink(join(folder, name), join(previous, name), "dir");
        await applyMigrations(url, previous);
      } finally {
        await rm(previous, { recursive: true });
      }

      const client = new pg.Client(local.settings(databaseName));
      await client.connect();
      try {
        const org = "00000000-0000-4000-8000-000000000001";
        const other = "00000000-0000-4000-8000-000000000002";
        const owner = "00000000-0000-4000-8000-000000000003";
        await client.query(
          "INSERT INTO organisations (id, provider, external_id) VALUES ($1, 'dev', 'fleet-one'), ($2, 'dev', 'fleet-two')",
          [org, other],
        );
        await client.query(
          "INSERT INTO users (id, organisation_id, subject, display_name) VALUES ($1, $2, 'admin@example.test', 'Admin')",
          [owner, org],
        );
        await client.query(
          `INSERT INTO fleets (organisation_id, slug, name, irc_host, irc_port, irc_tls, rear_admiral_name, captain_names)
        VALUES ($1, 'known', 'Known', 'irc.example.test', 6697, true, 'Old name', '["Former Captain"]'),
               ($2, 'unmatched', 'Other tenant', 'irc.example.test', 6697, true, 'Old name', '[]')`,
          [org, other],
        );
        await client.query(
          `INSERT INTO users (organisation_id, subject, display_name, agent_rank, agent_fleet, agent_ship, agent_model, registered_by_id)
        VALUES ($1, 'one', 'One', 'captain', ' KNOWN ', 'Aurora', 'fixture', $2),
               ($1, 'two', 'Two', 'captain', 'known', ' aurora ', 'fixture', $2),
               ($1, 'three', 'Three', 'captain', 'unmatched', 'Aurora', 'fixture', $2),
               ($1, 'four', 'Four', 'captain', 'known', E'\\t', 'fixture', $2)`,
          [org, owner],
        );

        const project = "00000000-0000-4000-8000-000000000004";
        const state = "00000000-0000-4000-8000-000000000005";
        await client.query(
          `INSERT INTO projects (id, organisation_id, key, name, lead_id, created_by_user_id, default_state_id)
        VALUES ($1, $2, 'OLD', 'Old project', $3, $3, $4)`,
          [project, org, owner, state],
        );
        await client.query(
          `INSERT INTO workflow_states (id, organisation_id, project_id, name, type, position, is_default)
        VALUES ($1, $2, $3, 'Todo', 'unstarted', 0, true)`,
          [state, org, project],
        );
        const {
          rows: [legacyDecision],
        } = await client.query(
          `INSERT INTO project_decisions (organisation_id, project_id, recorded_by_id, title, decided_on, decision)
        SELECT $1, $2, id, 'Legacy decision', DATE '2026-10-11', 'Example decision'
        FROM users WHERE organisation_id = $1 AND subject = 'one'
        RETURNING recorded_by_id`,
          [org, project],
        );

        const inspect = async () =>
          (
            await client.query(`SELECT subject, agent_fleet, agent_ship, agent_fleet_id, agent_ship_id
        FROM users WHERE agent_rank IS NOT NULL ORDER BY subject`)
          ).rows;
        await applyMigrations(url, folder);
        const {
          rows: [decision],
        } = await client.query(
          "SELECT made_by_type, made_by_user_id, made_by_agent_name, made_by_agent_rank FROM project_decisions",
        );
        expect(decision).toEqual({
          made_by_type: "agent",
          made_by_user_id: legacyDecision!.recorded_by_id,
          made_by_agent_name: "One",
          made_by_agent_rank: "captain",
        });
        expect(
          (
            await client.query(
              "SELECT rear_admiral_name, captain_names FROM fleets WHERE slug = 'known'",
            )
          ).rows,
        ).toEqual([{ rear_admiral_name: "Old name", captain_names: ["Former Captain"] }]);
        const links = await inspect();
        const four = links[0]!;
        const one = links[1]!;
        const three = links[2]!;
        const two = links[3]!;
        expect(one.agent_fleet_id).not.toBeNull();
        expect(one.agent_ship_id).not.toBeNull();
        expect(two.agent_ship_id).toBe(one.agent_ship_id);
        expect(two.agent_ship).toBe(" aurora ");
        expect(four.agent_fleet_id).toBe(one.agent_fleet_id);
        expect(four.agent_ship_id).toBeNull();
        expect(three).toMatchObject({
          agent_fleet_id: null,
          agent_ship_id: null,
          agent_fleet: "unmatched",
          agent_ship: "Aurora",
        });
        expect((await client.query("SELECT name FROM ships")).rows).toEqual([{ name: "Aurora" }]);
        await applyMigrations(url, folder);
        expect(await inspect()).toEqual(links);

        // Exercise the data statements even after the migration journal skips the second application.
        const sql = await Bun.file(join(folder, fleetMigration, "migration.sql")).text();
        await client.query(sql.slice(sql.indexOf("UPDATE users AS agent")));
        expect(await inspect()).toEqual(links);
      } finally {
        await client.end();
      }
    } finally {
      await admin.query(`DROP DATABASE IF EXISTS "${databaseName}" WITH (FORCE)`);
      await admin.end();
    }
  },
);
