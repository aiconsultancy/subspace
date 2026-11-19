import { expect, test } from "bun:test";
import { EffectDrizzleQueryError } from "drizzle-orm/effect-core/errors";
import { Effect, Logger } from "effect";
import { SqlError, UniqueViolation } from "effect/sql/SqlError";
import { handleDb } from "../src/middleware";

test("database failure logs include the underlying SQLSTATE and exclude private values", async () => {
  const privateValue = "private fixture value";
  const error = new EffectDrizzleQueryError({
    query: `select '${privateValue}'`,
    params: [privateValue],
    cause: new SqlError({
      reason: new UniqueViolation({
        cause: { code: "23505", detail: privateValue },
        message: privateValue,
        constraint: privateValue,
        operation: "query",
      }),
    }),
  });
  const logs: unknown[] = [];
  const result = await Effect.runPromise(
    handleDb(Effect.fail(error)).pipe(
      Effect.provide(Logger.layer([Logger.make(({ message }) => logs.push(message))])),
      Effect.flip,
    ),
  );
  expect(result).toMatchObject({ status: 500, detail: "The database operation failed." });
  expect(JSON.stringify(logs)).not.toContain(privateValue);
  expect(logs).toEqual([
    [
      "Database operation failed",
      {
        category: "UniqueViolation",
        sqlState: "23505",
        operation: "query",
      },
    ],
  ]);
});
