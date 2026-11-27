import { expect, test } from "bun:test";
import { mkdtemp, readdir, rm, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { migrate } from "drizzle-orm/effect-postgres/migrator";
import { Effect } from "effect";
import pg from "pg";
import { Database, databaseLayer } from "../src/db";
import { localDatabase as localServer } from "./local-database";

const testDatabase = process.env.TEST_DATABASE_URL;
const folder = fileURLToPath(new URL("../drizzle", import.meta.url));

function applyMigrations(url: string, migrationsFolder: string) {
  return Effect.runPromise(
    Effect.gen(function* () {
      const db = yield* Database;
      yield* migrate(db, { migrationsFolder });
    }).pipe(Effect.provide(databaseLayer(url))),
  );
}

test.skipIf(!testDatabase)(
  "ready-for-agents migrations default existing issues to false, index the poll, clear assigned flags and are safe to run twice",
  async () => {
    const server = localServer(testDatabase!);
    const databaseName = `ready_migration_${crypto.randomUUID().replaceAll("-", "")}`;
    const admin = new pg.Client(server.settings(server.database));
    await admin.connect();
    try {
      await admin.query(`CREATE DATABASE "${databaseName}"`);
      const url = server.urlFor(databaseName);
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

      const client = new pg.Client(server.settings(databaseName));
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

        const index = async () =>
          (
            await client.query(
              "SELECT indexdef FROM pg_indexes WHERE tablename = 'issues' AND indexname = 'issues_ready_for_agents_idx'",
            )
          ).rows.map(({ indexdef }) => String(indexdef));
        const [definition] = await index();
        expect(definition).toContain("(organisation_id)");
        expect(definition).toContain("ready_for_agents");
        expect(definition).toContain("assignee_id IS NULL");
        expect(definition).toContain("archived_at IS NULL");

        await applyMigrations(url, folder);
        expect(await inspect()).toEqual(migrated);
        // Run the statements again outside the journal, as a hand-applied rerun would.
        const indexMigration = directories.find((name) =>
          name.endsWith("_issue_ready_for_agents_index"),
        )!;
        for (const name of [readyMigration, indexMigration])
          await client.query(await Bun.file(join(folder, name, "migration.sql")).text());
        expect(await inspect()).toEqual(migrated);
        expect(await index()).toEqual([definition!]);

        // An older writer that assigns a flagged issue, or inserts one assigned and flagged,
        // still leaves it not ready.
        await client.query("UPDATE issues SET ready_for_agents = true WHERE identifier = 'OLD-1'");
        expect((await inspect())[0]).toMatchObject({ ready_for_agents: true });
        await client.query("UPDATE issues SET title = 'Renamed' WHERE identifier = 'OLD-1'");
        expect((await inspect())[0]).toMatchObject({ ready_for_agents: true });
        await client.query("UPDATE issues SET assignee_id = $1 WHERE identifier = 'OLD-1'", [
          owner,
        ]);
        expect((await inspect())[0]).toMatchObject({ ready_for_agents: false });
        await client.query("UPDATE issues SET ready_for_agents = true WHERE identifier = 'OLD-2'");
        expect((await inspect())[1]).toMatchObject({ ready_for_agents: false });
        await client.query(
          `INSERT INTO issues (organisation_id, project_id, number, identifier, title, summary, state_id, assignee_id, ready_for_agents, created_by_id)
        VALUES ($1, $2, 3, 'OLD-3', 'Inserted', 'Older writer', $3, $4, true, $4)`,
          [org, project, state, owner],
        );
        expect((await inspect())[2]).toMatchObject({
          identifier: "OLD-3",
          ready_for_agents: false,
        });
        await client.query(
          "UPDATE issues SET assignee_id = NULL, ready_for_agents = true WHERE identifier = 'OLD-3'",
        );
        expect((await inspect())[2]).toMatchObject({ ready_for_agents: true });
      } finally {
        await client.end();
      }
    } finally {
      await admin.query(`DROP DATABASE IF EXISTS "${databaseName}" WITH (FORCE)`);
      await admin.end();
    }
  },
);

test("the migration test only reaches a loopback server named by the URL itself", () => {
  for (const raw of [
    "postgresql://subspace@127.0.0.1:5432/subspace?host=db.example.com",
    "postgresql://subspace@localhost/subspace?hostaddr=192.0.2.10",
    "postgresql://subspace@localhost/subspace?sslmode=disable",
    "postgresql://subspace@db.example.com:5432/subspace",
    "postgresql:///subspace",
    "mysql://subspace@localhost/subspace",
  ])
    expect(() => localServer(raw)).toThrow();
  const server = localServer("postgresql://sub%40space:p%2Fss@[::1]:5433/sub%20space");
  expect(server.database).toBe("sub space");
  expect(server.settings("other")).toEqual({
    host: "::1",
    port: 5433,
    user: "sub@space",
    password: "p/ss",
    database: "other",
  });
  expect(server.urlFor("other")).toBe("postgresql://sub%40space:p%2Fss@[::1]:5433/other");
});
