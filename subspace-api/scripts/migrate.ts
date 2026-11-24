import { sql } from "drizzle-orm";
import { readMigrationFiles } from "drizzle-orm/migrator";
import { migrate } from "drizzle-orm/effect-postgres/migrator";
import { Effect } from "effect";
import { reservedUsernames } from "../src/config";
import { Database, databaseLayer, databaseUrl } from "../src/db";
import { backfillUsernames } from "../src/usernames";

const reserved = reservedUsernames(process.env);
const migrationsFolder = new URL("../drizzle", import.meta.url).pathname;
// `--check` changes nothing. It exits with this code when the database lacks migrations, so that an operator
// decides whether to migrate a database the API does not own (the fleet installer's --migrate).
const pendingExitCode = 3;

const migration = Effect.gen(function* () {
  const db = yield* Database;
  yield* db.transaction((tx) =>
    Effect.gen(function* () {
      // Concurrent API instances must wait before Drizzle inspects its journal.
      yield* tx.execute(sql`SELECT pg_advisory_xact_lock(hashtext('subspace-api-migrations'))`);
      yield* migrate(tx, { migrationsFolder });
    }),
  );
  // After the schema change has committed, so no lock on the users table is held meanwhile.
  // People need the operator's reserved names, which only the runtime configuration holds, so
  // the backfill runs here rather than in a SQL migration, in short transactions of its own.
  yield* backfillUsernames(db, reserved);
});

// Drizzle records each applied migration by name in drizzle.__drizzle_migrations and applies the rest.
const check = Effect.gen(function* () {
  const db = yield* Database;
  const local = readMigrationFiles({ migrationsFolder }).map((file) => file.name);
  const history = yield* db.execute<{ name: string | null }>(
    sql`SELECT to_regclass('drizzle.__drizzle_migrations')::text AS name`,
  );
  const applied = new Set<string>();
  if (history.rows[0]?.name) {
    const rows = yield* db.execute<{ name: string | null }>(
      sql`SELECT name FROM drizzle.__drizzle_migrations`,
    );
    for (const row of rows.rows) if (row.name) applied.add(row.name);
  }
  const pending = local.filter((name) => !applied.has(name)).length;
  const unknown = [...applied].filter((name) => !local.includes(name)).length;
  if (unknown)
    console.log(
      `The database has ${unknown} migration(s) this API does not know: it may be newer than this API.`,
    );
  if (!pending) {
    console.log("The database has every migration this API needs.");
    return 0;
  }
  console.log(
    applied.size
      ? `The database lacks ${pending} of this API's ${local.length} migrations.`
      : `The database has no migration history: all ${local.length} of this API's migrations are missing.`,
  );
  return pendingExitCode;
});

const checking = process.argv.includes("--check");
try {
  const layer = databaseLayer(databaseUrl());
  if (checking) process.exitCode = await Effect.runPromise(check.pipe(Effect.provide(layer)));
  else await Effect.runPromise(migration.pipe(Effect.provide(layer)));
} catch {
  console.error(
    checking
      ? "Could not read the database's migration history. Check the connection and the role's privileges."
      : "Database migration failed. Check the connection, migration files and database permissions.",
  );
  process.exitCode = 1;
}
