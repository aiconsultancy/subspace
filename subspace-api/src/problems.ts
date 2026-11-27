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
    instance: Schema.optional(Schema.String),
    correlationId: Schema.optional(Schema.String),
    errors: Schema.optional(Schema.Record(Schema.String, Schema.Array(Schema.String))),
    currentVersion: Schema.optional(Schema.Int),
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

/**
 * Stable problem types a client may branch on, in a problem's `type`. Every other problem is
 * `about:blank`, and its title and status are all it promises.
 */
export const problemTypes = {
  /** The request's body or fields are malformed (400). */
  invalidRequest: "urn:subspace:problem:invalid-request",
  /** A pairing invite that is unknown, malformed, expired, revoked or already used (401). */
  pairingInviteUnusable: "urn:subspace:problem:pairing-invite-unusable",
  /** A certificate that is already registered, or was revoked (409). */
  credentialConflict: "urn:subspace:problem:credential-conflict",
  /** This API does not pair devices: AUTH_PROVIDERS lacks pairing (404). */
  pairingUnavailable: "urn:subspace:problem:pairing-unavailable",
  /**
   * A provider token given to link an account that is not a fresh, valid WorkOS or Entra access
   * token for this API (400).
   */
  identityTokenUnusable: "urn:subspace:problem:identity-token-unusable",
  /** A provider account that is linked to, or registered as, another member (409). */
  identityConflict: "urn:subspace:problem:identity-conflict",
  /**
   * An administrator's invite for a person who already signs in, with a provider account or a
   * paired device, and pairs another device themselves (409).
   */
  memberSignsIn: "urn:subspace:problem:member-signs-in",
} as const;
type ProblemType = (typeof problemTypes)[keyof typeof problemTypes];
const knownTypes: ReadonlySet<string> = new Set(Object.values(problemTypes));

/** A problem with one of the stable types, which a client may branch on. */
export function typedProblem<Status extends ProblemStatus>(
  status: Status,
  type: ProblemType,
  detail: string,
) {
  return { ...problem(status, detail), type };
}

/*
 * The typed refusals of the signed-in pairing and linking routes. Their other problems, including
 * the shared 400 for a malformed body, are `about:blank`; each route's description names its typed
 * ones.
 */

/** The refusal of a pairing route when AUTH_PROVIDERS lacks pairing (404). */
export const pairingUnavailable = () =>
  typedProblem(404, problemTypes.pairingUnavailable, "This API does not pair devices.");

/** An administrator's invite for a person who already signs in (409). */
export const memberSignsIn = () =>
  typedProblem(
    409,
    problemTypes.memberSignsIn,
    "This person already signs in, with a provider account or a paired device. They pair another device themselves, from /me/pairing/invites.",
  );

/** A provider token that cannot link an account (400). */
export const identityTokenUnusable = (detail: string) =>
  typedProblem(400, problemTypes.identityTokenUnusable, detail);

/** A provider account, or provider organisation, that is another member's or organisation's (409). */
export const identityConflict = (detail: string) =>
  typedProblem(409, problemTypes.identityConflict, detail);

/** Whether a failure is a problem this API answers with, rather than an unexpected error. */
export const isProblem = (error: unknown): error is { type: string; status: number } =>
  typeof error === "object" &&
  error !== null &&
  "type" in error &&
  typeof error.type === "string" &&
  (error.type === "about:blank" || knownTypes.has(error.type));

export function validationProblem(field: string, message: string) {
  return {
    ...problem(400, "One or more validation errors occurred. Correct the listed fields."),
    errors: { [field]: [message] },
  };
}
