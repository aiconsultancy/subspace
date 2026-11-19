import { sql } from "drizzle-orm";
import { migrate } from "drizzle-orm/effect-postgres/migrator";
import { Effect } from "effect";
import { Database, databaseLayer, databaseUrl } from "../src/db";

const migration = Effect.gen(function* () {
  const db = yield* Database;
  yield* db.transaction((tx) =>
    Effect.gen(function* () {
      // Concurrent API instances must wait before Drizzle inspects its journal.
      yield* tx.execute(sql`SELECT pg_advisory_xact_lock(hashtext('subspace-api-migrations'))`);
      yield* migrate(tx, { migrationsFolder: new URL("../drizzle", import.meta.url).pathname });
    }),
  );
});

try {
  await Effect.runPromise(migration.pipe(Effect.provide(databaseLayer(databaseUrl()))));
} catch {
  console.error(
    "Database migration failed. Check the connection, migration files and database permissions.",
  );
  process.exitCode = 1;
}
