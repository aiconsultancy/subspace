import { and, eq, sql } from "drizzle-orm";
import { Effect, Layer, Schema } from "effect";
import { HttpServerRequest, HttpServerResponse } from "effect/http";
import {
  HttpApi,
  HttpApiEndpoint,
  HttpApiError,
  HttpApiGroup,
  HttpApiMiddleware,
  HttpApiSchema,
  OpenApi,
} from "effect/http-api";
import { insertCredential, lockOrganisation, readCertificate } from "./agent-credentials";
import {
  agentTokenMaxLifetime,
  CertificatePem,
  CredentialScope,
  deviceTokenType,
  KeyAlgorithm,
} from "./agent-keys";
import { recordAudit } from "./audit";
import { deviceTokenIssuer, externalClockTolerance } from "./auth";
import { type Provider } from "./config";
import { Database } from "./db";
import { hasConstraint } from "./fleets";
import { type Member } from "./identity";
import { handleDb, versionedApi } from "./middleware";
import { sameHash } from "./one-time-tokens";
import { issuerRefusal, pairingInvitePrefix, readPairingInvite } from "./pairing-invites";
import {
  BadRequest,
  endpointErrors,
  InternalError,
  isProblem,
  pairingUnavailable,
  problem,
  problemTypes,
  typedProblem,
  Unauthorised,
} from "./problems";
import { organisations, pairingInvites, users } from "./schema";
import { Line, Uuid } from "./validation";

/**
 * Workstation pairing's unauthenticated contract: the capability probe, and redemption, by which a
 * person's device registers its own key and self-signed certificate as one of that member's device
 * credentials with a single-use invite (see `pairing-invites.ts`). The device then signs device
 * tokens with the key, as an agent signs agent tokens (see `agent-keys.ts`). The owner bootstrap
 * command's work is in `pairing-owner.ts`.
 */
export const pairingPath = `${versionedApi}/pairing`;
export const redeemPath = `${pairingPath}/redeem`;
/**
 * A paired device signs API requests and may ask for an IRC password, which is bound to it: see
 * `irc.ts`. It never signs in to IRC with its certificate, which only an agent's key may.
 */
const deviceScopes = ["api", "irc"] as const;

/** What paired devices sign their tokens for: the audience, and whether pairing is on at all. */
export interface PairingPolicy {
  /** The device token audience, or null when pairing is off (AUTH_PROVIDERS lacks pairing). */
  readonly audience: string | null;
  /** The providers of the organisations this deployment serves. */
  readonly providers: ReadonlySet<Provider>;
  /** The Entra app role that makes a person an administrator, or null (ENTRA_ADMIN_ROLE). */
  readonly entraAdminRole: string | null;
}

/** What a device signs its tokens with, so that a client need not hard-code it. */
const DeviceTokenProfile = Schema.Struct({
  type: Schema.Literal(deviceTokenType),
  issuer: Schema.Literal(deviceTokenIssuer),
  audience: Schema.String,
  algorithms: Schema.Array(KeyAlgorithm),
  maxLifetimeSeconds: Schema.Int,
  clockToleranceSeconds: Schema.Int,
}).annotate({ identifier: "DeviceTokenProfile" });

const deviceTokenProfile = (audience: string) =>
  ({
    type: deviceTokenType,
    issuer: deviceTokenIssuer,
    audience,
    // Ed25519 first: it is the algorithm every client implements.
    algorithms: ["EdDSA", "ES256"],
    maxLifetimeSeconds: agentTokenMaxLifetime,
    clockToleranceSeconds: externalClockTolerance,
  }) as const;

const PairingCapabilities = Schema.Struct({
  enabled: Schema.Boolean,
  inviteVersion: Schema.Literal("spair1"),
  redeemPath: Schema.Literal(redeemPath),
  deviceToken: Schema.NullOr(DeviceTokenProfile),
}).annotate({ identifier: "PairingCapabilities" });

const maxInviteLength = 256;
const Redemption = Schema.Struct({
  /** The invite, as one line: never in a URL, a query or a header that a proxy logs. */
  invite: Schema.String.check(Schema.isMaxLength(maxInviteLength)),
  certificate: CertificatePem,
  label: Schema.optional(Schema.NullOr(Line(120))),
}).annotate({ identifier: "PairingRedemption" });

const DevicePaired = Schema.Struct({
  memberId: Uuid,
  organisationId: Uuid,
  credentialId: Uuid,
  fingerprint: Schema.String,
  scopes: Schema.Array(CredentialScope),
  deviceToken: DeviceTokenProfile,
}).annotate({ identifier: "DevicePaired" });

const unusableDetail = "This pairing invite cannot be used. Ask for a new invite.";

/**
 * The one refusal of an invite, whatever the reason, so that its answer tells nothing about
 * which invites exist. The reason goes to a debug log line; the invite never does.
 */
const unusable = (reason: string) =>
  Effect.logDebug("Pairing invite refused", { reason }).pipe(
    Effect.andThen(
      Effect.fail(typedProblem(401, problemTypes.pairingInviteUnusable, unusableDetail)),
    ),
  );

/**
 * Why a stored invite no longer pairs anything, if it does not. Whether it has expired is the
 * database's answer, not this process's clock: see `redeem`.
 */
function spent(row: typeof pairingInvites.$inferSelect, expired: boolean) {
  if (row.revokedAt) return "revoked";
  if (row.redeemedAt) return "used";
  if (expired) return "expired";
  return undefined;
}

/**
 * Registers the certificate as a device credential of the invite's member and spends the invite,
 * in one transaction under the organisation's lock and the invite's row lock, so that however
 * many requests race, an invite registers one credential. The member must be an active person of
 * an organisation this deployment serves. An invite whose secret does not match is refused on an
 * unlocked read, before any transaction or lock: a caller who knows only an invite's row ID can
 * neither learn from a lock wait that the row exists nor contend with real writes.
 */
const redeem = (policy: PairingPolicy, payload: typeof Redemption.Type) =>
  Effect.gen(function* () {
    const { audience } = policy;
    if (!audience) return yield* Effect.fail(pairingUnavailable());
    const read = readPairingInvite(payload.invite);
    if (!read) return yield* unusable("shape");
    const certificate = yield* readCertificate(payload.certificate).pipe(
      Effect.mapError((refusal) => ({ ...refusal, type: problemTypes.invalidRequest })),
    );
    const db = yield* Database;
    const [found] = yield* db
      .select({
        organisationId: pairingInvites.organisationId,
        tokenHash: pairingInvites.tokenHash,
      })
      .from(pairingInvites)
      .where(eq(pairingInvites.id, read.id));
    if (!found || !sameHash(read.hash, found.tokenHash)) return yield* unusable("unknown");
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        // Read again under the locks: the invite may have been spent or revoked since. Expiry is
        // judged by the database's clock once the organisation is locked, never by this process's,
        // so replicas whose clocks differ agree: once one refuses an invite as expired, no
        // redemption that takes the lock later can pair with it.
        yield* lockOrganisation(tx, found.organisationId);
        const [locked] = yield* tx
          .select({
            invite: pairingInvites,
            expired: sql<boolean>`${pairingInvites.expiresAt} <= clock_timestamp()`,
          })
          .from(pairingInvites)
          .where(
            and(
              eq(pairingInvites.organisationId, found.organisationId),
              eq(pairingInvites.id, read.id),
            ),
          )
          .for("update");
        if (!locked || !sameHash(read.hash, locked.invite.tokenHash))
          return yield* unusable("unknown");
        const { invite } = locked;
        const reason = spent(invite, locked.expired);
        if (reason) return yield* unusable(reason);
        const [row] = yield* tx
          .select({ member: users, provider: organisations.provider })
          .from(users)
          .innerJoin(organisations, eq(organisations.id, users.organisationId))
          .where(and(eq(users.organisationId, invite.organisationId), eq(users.id, invite.userId)))
          .for("share", { of: users });
        if (!row || row.member.agentRank || !row.member.active) return yield* unusable("member");
        if (!policy.providers.has(row.provider)) return yield* unusable("organisation");
        const withdrawn = yield* issuerRefusal(tx, invite, policy.entraAdminRole);
        if (withdrawn) return yield* unusable(withdrawn);
        const member: Member = row.member;

        const credential = yield* insertCredential(tx, {
          organisationId: invite.organisationId,
          userId: member.id,
          certificate,
          scopes: deviceScopes,
          label: payload.label ?? null,
          pairingInviteId: invite.id,
        }).pipe(
          // Its only problem: the certificate is already registered, or was revoked (409).
          Effect.mapError((error) =>
            isProblem(error) ? { ...error, type: problemTypes.credentialConflict } : error,
          ),
          Effect.catchIf(
            (error) => hasConstraint(error, "agent_credentials_pairing_invite"),
            () => unusable("used"),
          ),
        );
        // Dated by the database's clock, which judged the invite unexpired above.
        yield* tx
          .update(pairingInvites)
          .set({ redeemedAt: sql`clock_timestamp()` })
          .where(eq(pairingInvites.id, invite.id));
        yield* recordAudit(tx, member, {
          action: "deviceCredentialAdded",
          entityType: "DeviceCredential",
          entityId: credential.id,
          summary: "Paired a device with a pairing invite.",
          changes: {
            memberId: member.id,
            fingerprint: credential.fingerprint,
            scopes: credential.scopes,
            pairingInviteId: invite.id,
          },
        });
        yield* recordAudit(tx, member, {
          action: "pairingInviteRedeemed",
          entityType: "PairingInvite",
          entityId: invite.id,
          summary: "Redeemed a pairing invite.",
          changes: { memberId: member.id, credentialId: credential.id },
        });
        return {
          memberId: member.id,
          organisationId: member.organisationId,
          credentialId: credential.id,
          fingerprint: credential.fingerprint,
          scopes: deviceScopes,
          deviceToken: deviceTokenProfile(audience),
        };
      }),
    );
  });

class PairingProblems extends HttpApiMiddleware.Service<PairingProblems>()(
  "subspace/PairingProblems",
  { error: [BadRequest, InternalError] },
) {}

const invalidRequest = (detail: string) => typedProblem(400, problemTypes.invalidRequest, detail);
const notJson = () => invalidRequest("Send an application/json body.");

/**
 * The pairing group's refusals of a request's shape, typed so that clients can branch on them; the
 * detail never repeats a field's value. A body is JSON only. Effect reads a body without a
 * Content-Type as JSON, so that is refused here; for another media type it answers 415, not a
 * schema failure, when it cannot select a body decoder.
 */
export const pairingProblems = Layer.succeed(PairingProblems, (handle, { endpoint }) =>
  Effect.gen(function* () {
    const request = yield* HttpServerRequest.HttpServerRequest;
    if (endpoint.payload.size && !request.headers["content-type"]?.trim())
      return yield* Effect.fail(notJson());
    const response = yield* handle;
    return response.status === 415 ? yield* Effect.fail(notJson()) : response;
  }).pipe(
    Effect.catch((error) =>
      Effect.fail(
        !HttpApiError.HttpApiSchemaError.is(error)
          ? error
          : error.kind === "ResponseHeaders"
            ? problem(500, "The response could not be serialised.")
            : invalidRequest("The request contains invalid fields."),
      ),
    ),
  ),
);

/**
 * Pairing on the main listener, outside the bearer middleware: the probe needs no token, and a
 * redemption is authorised by the invite in its body alone.
 */
export const pairingGroup = HttpApiGroup.make("pairing")
  .add(
    HttpApiEndpoint.get("pairingCapabilities", "/pairing", {
      success: PairingCapabilities,
    }).annotate(
      OpenApi.Description,
      "Whether this API pairs devices, and the device token profile a paired device signs with. It needs no token and tells nothing secret, so a client can check it before it sends an invite: an API that predates pairing answers 404, and one that does not pair devices answers enabled false.",
    ),
    HttpApiEndpoint.post("redeemPairingInvite", "/pairing/redeem", {
      payload: Redemption,
      success: DevicePaired.pipe(HttpApiSchema.status(201)),
      error: [...endpointErrors, Unauthorised],
    }).annotate(
      OpenApi.Description,
      [
        `A person's device registers its own certificate as one of the invite's member's device credentials, and spends the invite (201). The invite, which begins ${pairingInvitePrefix}, is sent in the JSON body only.`,
        `Problems carry a stable type: ${problemTypes.invalidRequest} (400) for a missing or unsupported content type, malformed body or fields (including an invite field longer than ${maxInviteLength} characters), or a certificate that is not Ed25519 or P-256;`,
        `${problemTypes.pairingInviteUnusable} (401) for an otherwise valid request with an unknown, malformed, expired, revoked or used invite, or one whose member can no longer pair, all with the same detail;`,
        `${problemTypes.credentialConflict} (409) for a certificate already registered or revoked; ${problemTypes.pairingUnavailable} (404) when this API does not pair devices.`,
        "A redemption retry with the same invite returns 401 if the first attempt committed. While the outcome is unknown, keep the pending key and certificate.",
        "While the invite may still be usable, retry with the same invite and certificate, then check with a freshly signed GET /api/v1/me. A 200 confirms pairing. Even a second /me 401 does not rule out an in-flight commit.",
        "Ask for a new invite only after the original invite is refused as unusable and a fresh /me still returns 401 with the audience and clock checked, or after an administrator confirms the original invite cannot be redeemed and the pending device is not active. Otherwise keep the pending record.",
        "Asking for a replacement invite never permits discarding a pending key whose outcome is still unknown; pair the replacement with a new key.",
        "Expiry is judged by the database's clock, so every API replica gives the same answer.",
      ].join(" "),
    ),
  )
  .annotate(HttpApi.PayloadParseOptions, { onExcessProperty: "error" })
  .prefix(versionedApi)
  .middleware(PairingProblems);

export const pairingHandlers =
  (policy: PairingPolicy) =>
  (handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof pairingGroup>) =>
    handlers
      .handle("pairingCapabilities", () =>
        Effect.succeed({
          enabled: policy.audience !== null,
          inviteVersion: "spair1" as const,
          redeemPath,
          deviceToken: policy.audience ? deviceTokenProfile(policy.audience) : null,
        }),
      )
      .handle("redeemPairingInvite", ({ payload }) =>
        handleDb(redeem(policy, payload)).pipe(
          Effect.map((body) =>
            HttpServerResponse.jsonUnsafe(body, {
              status: 201,
              headers: { "Cache-Control": "no-store" },
            }),
          ),
        ),
      );
