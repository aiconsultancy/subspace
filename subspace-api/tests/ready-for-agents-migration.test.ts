import { expect, test } from "bun:test";
import { mkdtemp, readdir, rm, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { migrate } from "drizzle-orm/effect-postgres/migrator";
import { Effect } from "effect";
import pg from "pg";
import { Database, databaseLayer } from "../src/db";

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
  "ready-for-agents migration defaults existing issues to false and is safe to run twice",
  async () => {
    const connection = new URL(testDatabase!);
    if (!["localhost", "127.0.0.1", "[::1]"].includes(connection.hostname))
      throw new Error("TEST_DATABASE_URL must point to a local test database.");
    const databaseName = `ready_migration_${crypto.randomUUID().replaceAll("-", "")}`;
    const admin = new pg.Client(testDatabase);
    await admin.connect();
    try {
      await admin.query(`CREATE DATABASE "${databaseName}"`);
      connection.pathname = databaseName;
      const url = connection.toString();
      const previous = await mkdtemp(join(tmpdir(), "subspace-drizzle-"));
      const directories = (await readdir(folder)).sort();
      const readyMigration = directories.find((name) => name.endsWith("_issue_ready_for_agents"))!;
      expect(readyMigration).toBeDefined();
      try {
        for (const name of directories.filter((name) => name < readyMigration))
          await symlink(join(folder, name), join(previous, name), "dir");
        await applyMigrations(url, previous);
      } finally {
        await rm(previous, { recursive: true });
      }

      const client = new pg.Client(url);
      await client.connect();
      try {
        const org = "00000000-0000-4000-8000-000000000001";
        const owner = "00000000-0000-4000-8000-000000000002";
        const project = "00000000-0000-4000-8000-000000000003";
        const state = "00000000-0000-4000-8000-000000000004";
        await client.query(
          "INSERT INTO organisations (id, provider, external_id) VALUES ($1, 'dev', 'ready-one')",
          [org],
        );
        await client.query(
          "INSERT INTO users (id, organisation_id, subject, display_name) VALUES ($1, $2, 'admin@example.test', 'Admin')",
          [owner, org],
        );
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
        await client.query(
          `INSERT INTO issues (organisation_id, project_id, number, identifier, title, summary, state_id, assignee_id, created_by_id)
        VALUES ($1, $2, 1, 'OLD-1', 'Unassigned', 'Existing issue', $3, NULL, $4),
               ($1, $2, 2, 'OLD-2', 'Assigned', 'Existing issue', $3, $4, $4)`,
          [org, project, state, owner],
        );

        const inspect = async () =>
          (
            await client.query(
              "SELECT identifier, ready_for_agents, version FROM issues ORDER BY identifier",
            )
          ).rows;
        await applyMigrations(url, folder);
        const migrated = await inspect();
        expect(migrated).toEqual([
          { identifier: "OLD-1", ready_for_agents: false, version: 1 },
          { identifier: "OLD-2", ready_for_agents: false, version: 1 },
        ]);
        const {
          rows: [column],
        } = await client.query(
          `SELECT is_nullable, column_default FROM information_schema.columns
        WHERE table_name = 'issues' AND column_name = 'ready_for_agents'`,
        );
        expect(column).toEqual({ is_nullable: "NO", column_default: "false" });

        await applyMigrations(url, folder);
        expect(await inspect()).toEqual(migrated);
        // Run the statement again outside the journal, as a hand-applied rerun would.
        await client.query(await Bun.file(join(folder, readyMigration, "migration.sql")).text());
        expect(await inspect()).toEqual(migrated);
      } finally {
        await client.end();
      }
    } finally {
      await admin.query(`DROP DATABASE IF EXISTS "${databaseName}" WITH (FORCE)`);
      await admin.end();
    }
  },
);
