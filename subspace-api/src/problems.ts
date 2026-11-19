import { Schema } from "effect";
import { HttpApiSchema } from "effect/http-api";

const titles = {
  400: "Bad Request",
  401: "Unauthorized",
  403: "Forbidden",
  404: "Not Found",
  409: "Conflict",
  410: "Gone",
  429: "Too Many Requests",
  500: "Internal Server Error",
  503: "Service Unavailable",
} as const;

type ProblemStatus = keyof typeof titles;

const problemSchema = <Status extends ProblemStatus>(status: Status) =>
  Schema.Struct({
    type: Schema.String,
    title: Schema.Literal(titles[status]),
    status: Schema.Literal(status),
    detail: Schema.String,
    currentVersion: Schema.optional(Schema.Int),
    errors: Schema.optional(Schema.Record(Schema.String, Schema.Array(Schema.String))),
  })
    .annotate({ identifier: `Problem${status}` })
    .pipe(
      HttpApiSchema.status(status),
      HttpApiSchema.asJson({ contentType: "application/problem+json" }),
    );

export const BadRequest = problemSchema(400);
export const Unauthorised = problemSchema(401);
export const Forbidden = problemSchema(403);
export const Missing = problemSchema(404);
export const Conflict = problemSchema(409);
export const Gone = problemSchema(410);
export const TooManyRequests = problemSchema(429);
export const InternalError = problemSchema(500);
export const Unavailable = problemSchema(503);

export const endpointErrors = [
  BadRequest,
  Forbidden,
  Missing,
  Conflict,
  Gone,
  Unavailable,
  InternalError,
] as const;

export function problem<Status extends ProblemStatus>(
  status: Status,
  detail: string,
  currentVersion?: number,
) {
  return { type: "about:blank", title: titles[status], status, detail, currentVersion, errors: {} };
}
