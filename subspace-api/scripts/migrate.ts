import { sql } from "drizzle-orm";
import { migrate } from "drizzle-orm/effect-postgres/migrator";
import { Effect } from "effect";
import { reservedUsernames } from "../src/config";
import { Database, databaseLayer, databaseUrl } from "../src/db";
import { backfillUsernames } from "../src/usernames";

const reserved = reservedUsernames(process.env);
const migration = Effect.gen(function* () {
  const db = yield* Database;
  yield* db.transaction((tx) =>
    Effect.gen(function* () {
      // Concurrent API instances must wait before Drizzle inspects its journal.
      yield* tx.execute(sql`SELECT pg_advisory_xact_lock(hashtext('subspace-api-migrations'))`);
      yield* migrate(tx, { migrationsFolder: new URL("../drizzle", import.meta.url).pathname });
    }),
  );
  // After the schema change has committed, so no lock on the users table is held meanwhile.
  // People need the operator's reserved names, which only the runtime configuration holds, so
  // the backfill runs here rather than in a SQL migration, in short transactions of its own.
  yield* backfillUsernames(db, reserved);
});

try {
  await Effect.runPromise(migration.pipe(Effect.provide(databaseLayer(databaseUrl()))));
} catch {
  console.error(
    "Database migration failed. Check the connection, migration files and database permissions.",
  );
  process.exitCode = 1;
}
