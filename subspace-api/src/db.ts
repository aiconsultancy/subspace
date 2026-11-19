import { PgClient } from "@effect/sql-pg";
import * as PgDrizzle from "drizzle-orm/effect-postgres";
import { Context, Layer, Redacted } from "effect";

export class Database extends Context.Service<Database, PgDrizzle.EffectPgDatabase>()(
  "subspace/Database",
) {}

export function databaseLayer(url: string, maxConnections = 10) {
  return Layer.effect(Database, PgDrizzle.makeWithDefaults()).pipe(
    Layer.provideMerge(
      PgClient.layer({
        url: Redacted.make(url),
        maxConnections,
      }),
    ),
  );
}

export function databaseUrl(env: NodeJS.ProcessEnv = process.env) {
  const url = env.DATABASE_URL;
  if (!url) throw new Error("DATABASE_URL is required");
  return url;
}
