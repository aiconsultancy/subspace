import { EffectDrizzleQueryError } from "drizzle-orm/effect-core/errors";
import { Effect } from "effect";
import { isSqlError } from "effect/sql/SqlError";
import { HttpApiMiddleware, HttpApiSecurity } from "effect/http-api";
import { CurrentMember } from "./identity";
import {
  BadRequest,
  endpointErrors,
  Forbidden,
  InternalError,
  problem,
  Unauthorised,
} from "./problems";

export const versionedApi = "/api/v1";

export class BearerAuth extends HttpApiMiddleware.Service<
  BearerAuth,
  { provides: CurrentMember }
>()("subspace/BearerAuth", {
  security: { bearer: HttpApiSecurity.bearer },
  error: [Unauthorised, Forbidden, InternalError],
}) {}

export class ValidationErrors extends HttpApiMiddleware.Service<ValidationErrors>()(
  "subspace/ValidationErrors",
  { error: [BadRequest, InternalError] },
) {}

export const validationErrors = HttpApiMiddleware.layerSchemaErrorTransform(
  ValidationErrors,
  (error) =>
    Effect.fail(
      error.kind === "ResponseHeaders"
        ? problem(500, "The response could not be serialised.")
        : problem(400, "The request contains invalid fields."),
    ),
);

type DomainProblem = (typeof endpointErrors)[number]["Type"];

function isDomainProblem(error: unknown): error is DomainProblem {
  return (
    typeof error === "object" && error !== null && "type" in error && error.type === "about:blank"
  );
}

function databaseFailure(error: unknown) {
  const cause = error instanceof EffectDrizzleQueryError ? error.cause : error;
  if (!isSqlError(cause)) {
    return { category: cause instanceof TypeError ? "TypeError" : "UnexpectedError" };
  }
  const reason = cause.reason;
  const underlying = reason.cause;
  const code =
    typeof underlying === "object" && underlying !== null && "code" in underlying
      ? underlying.code
      : undefined;
  // SQL messages, query parameters and driver details can contain credentials or user data.
  return {
    category: reason._tag,
    sqlState: typeof code === "string" && /^[0-9A-Z]{5}$/u.test(code) ? code : undefined,
    operation:
      reason.operation === "query" ||
      reason.operation === "connect" ||
      reason.operation === "commit" ||
      reason.operation === "acquireConnection"
        ? reason.operation
        : undefined,
  };
}

export function handleDb<A, E, R>(operation: Effect.Effect<A, E, R>) {
  return Effect.catch(operation, (error): Effect.Effect<never, DomainProblem> =>
    isDomainProblem(error)
      ? Effect.fail(error)
      : Effect.logError("Database operation failed", databaseFailure(error)).pipe(
          Effect.andThen(Effect.fail(problem(500, "The database operation failed."))),
        ),
  );
}
