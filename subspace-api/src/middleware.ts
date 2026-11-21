import { EffectDrizzleQueryError } from "drizzle-orm/effect-core/errors";
import { Effect, SchemaIssue } from "effect";
import { isSqlError } from "effect/sql/SqlError";
import { HttpServerRequest } from "effect/http";
import { HttpApiMiddleware, HttpApiSecurity } from "effect/http-api";
import { CurrentMember } from "./identity";
import {
  BadRequest,
  endpointErrors,
  Forbidden,
  InternalError,
  problem,
  Unauthorised,
  validationProblem,
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

const formatIssues = SchemaIssue.makeFormatterStandardSchemaV1();
const pagedResources = new Set(["agents", "fleets", "decisions"]);

export const validationErrors = HttpApiMiddleware.layerSchemaErrorTransform(
  ValidationErrors,
  (error, { group, endpoint }) =>
    Effect.gen(function* () {
      const request = yield* HttpServerRequest.HttpServerRequest;
      const url = new URL(request.url, "http://localhost");
      const invalid = formatIssues(error.cause.issue).issues[0]?.path?.[0];
      if (error.kind === "Query" && pagedResources.has(group.identifier)) {
        const value = typeof invalid === "string" ? url.searchParams.get(invalid) : null;
        if (invalid === "limit" && value !== null) {
          const limit = Number(value);
          const message =
            Number.isInteger(limit) && (limit < 1 || limit > 200)
              ? "Use a limit from 1 to 200 (default 50)."
              : `Value [${value}] is not valid for a [Int32] property!`;
          return yield* Effect.fail(validationProblem(invalid, message));
        }
        if (group.identifier === "decisions" && value !== null && typeof invalid === "string") {
          if (invalid === "madeByType" || invalid === "status") {
            const type = invalid === "madeByType" ? "DecisionMakerType" : "ProjectDecisionStatus[]";
            return yield* Effect.fail(
              validationProblem(invalid, `Value [${value}] is not valid for a [${type}] property!`),
            );
          }
          if (value.includes("\0"))
            return yield* Effect.fail(
              validationProblem(invalid, `Remove the NUL character from ${invalid}.`),
            );
        }
      }
      if (error.kind === "Params" && pagedResources.has(group.identifier)) {
        const field = group.identifier === "decisions" && invalid === "id" ? "decisionId" : invalid;
        if (
          field === "decisionId" ||
          field === "agentId" ||
          field === "fleetId" ||
          field === "shipId"
        ) {
          const segment =
            field === "fleetId" ? url.pathname.split("/").at(-2) : url.pathname.split("/").at(-1);
          return yield* Effect.fail(
            validationProblem(field, `Value [${segment}] is not valid for a [Guid] property!`),
          );
        }
      }
      if (
        error.kind === "Payload" &&
        (endpoint.identifier === "createShip" || endpoint.identifier === "renameShip")
      ) {
        if (invalid === "name")
          return yield* Effect.fail(
            validationProblem(
              "name",
              "Use 1 to 64 characters on one line without surrounding whitespace.",
            ),
          );
        if (invalid === "version")
          return yield* Effect.fail(validationProblem("version", "Use a positive version."));
      }
      return yield* Effect.fail(
        error.kind === "ResponseHeaders"
          ? problem(500, "The response could not be serialised.")
          : problem(400, "The request contains invalid fields."),
      );
    }),
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
