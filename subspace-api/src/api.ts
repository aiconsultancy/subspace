import { BunHttpServer } from "@effect/platform-bun";
import { sql } from "drizzle-orm";
import { Effect, Layer, Redacted, Schedule, Schema, type Types } from "effect";
import { HttpRouter, HttpServerRequest, HttpServerResponse } from "effect/http";
import {
  HttpApi,
  HttpApiBuilder,
  HttpApiEndpoint,
  HttpApiGroup,
  HttpApiSchema,
  OpenApi,
} from "effect/http-api";
import { agentCredentialGroup, agentCredentialHandlers } from "./agent-credentials";
import {
  enrolmentAuth,
  enrolmentGroup,
  enrolmentHandlers,
  enrolmentCheckPath,
  enrolmentPath,
  enrolmentTokenGroup,
  enrolmentTokenHandlers,
} from "./enrolment";
import { purgeAgentTokenIds, verifyAgentToken, type AgentTokenPolicy } from "./agent-keys";
import { agentGroup, agentHandlers } from "./agents";
import { auditGroup, auditHandlers } from "./audit";
import {
  agentTokenIssuer,
  devTokenLifetime,
  jwtVerifiers,
  mintDevToken,
  unverifiedIssuer,
  type RemoteFetch,
} from "./auth";
import { commentGroup, commentHandlers } from "./comment-mutations";
import { type Config } from "./config";
import { documentGroup, documentHandlers } from "./documents";
import { reviewGroup, reviewHandlers } from "./reviews";
import { cycleGroup, cycleHandlers } from "./cycles";
import { decisionGroup, decisionHandlers } from "./decisions";
import {
  acknowledgeDecisionOutcome,
  DecisionOutcome,
  listDecisionOutcomes,
} from "./decision-outcomes";
import { Database, databaseLayer } from "./db";
import {
  CreateFleet,
  CreateShip,
  RenameShip,
  Ship,
  createShip,
  listShips,
  renameShip,
  Fleet,
  FleetAssignment,
  FleetRef,
  UpdateFleet,
  createFleet,
  getFleet,
  listFleets,
  updateFleet,
} from "./fleets";
import {
  githubEventGroup,
  githubEventHandlers,
  githubGroup,
  githubHandlers,
  GithubIntegration,
} from "./github";
import { createGitHubClient } from "./github-app";
import { GithubPollState, githubPollState, pollGitHub } from "./github-poll";
import { AgentRank, CurrentMember, localMember, publicProfile } from "./identity";
import { ircAuthenticateLayer, ircCredentialGroup, ircCredentialHandlers } from "./irc";
import { signedInMember, type SignInPolicy } from "./registration";
import { ReservedUsernames } from "./usernames";
import {
  ChannelChanges,
  ChannelConfiguration,
  CreateInitiative,
  InitiativeDetail,
  InitiativePage,
  InitiativeQuery,
  MemberInitiative,
  UpdateInitiative,
  Version,
} from "./initiative-contract";
import {
  assignFleet,
  createInitiative,
  getInitiative,
  listInitiativeProjects,
  listInitiatives,
  memberInitiatives,
  updateChannels,
  updateInitiative,
} from "./initiatives";
import { endpointErrors, problem, Unavailable } from "./problems";
import { Email, Name, NullableString, PageQuery, pageOf, Slug, Uuid } from "./validation";

import {
  BearerAuth,
  databaseFailure,
  handleDb,
  validationErrors,
  ValidationErrors,
  versionedApi as prefix,
} from "./middleware";
import { planningGroup, planningHandlers } from "./planning";
import { Project } from "./projects";
import { issueGroup, issuesHandlers, projectGroup, projectsHandlers } from "./issues-http";

import { mcpGroup, mcpHandlersWithout } from "./mcp";
import {
  adminOnlyTools,
  agentAuth,
  agentDirectory,
  AgentDirectory,
  agentRequestGuard,
  withoutAdminEndpoints,
  type AgentListener,
} from "./agent-listener";
import { notificationsGroup, notificationHandlers } from "./notifications";
import { mediaGroup, mediaHandlers } from "./media-http";
import { purgeUploads } from "./media";
import { stakeholderGroup, stakeholderHandlers } from "./stakeholders";
import { Storage, storage } from "./storage";
import { todoGroup, todoHandlers } from "./todos";
import { viewGroup, viewHandlers } from "./views";
import { workflowGroup, workflowHandlers } from "./workflow";
import { updateHubLayer, updatesGroup, updatesHandlers } from "./updates";

const noStore = HttpServerResponse.setHeader("Cache-Control", "private, no-store");

const healthGroup = HttpApiGroup.make("health").add(
  HttpApiEndpoint.get("health", "/health", {
    success: Schema.Struct({ status: Schema.Literal("ok") }),
    error: Unavailable,
  }),
);

const identityGroup = HttpApiGroup.make("identity")
  .add(
    HttpApiEndpoint.get("me", "/me", {
      success: Schema.Struct({
        userId: Uuid,
        kind: Schema.Literals(["human", "agent"]),
        displayName: Name,
        email: NullableString,
        username: NullableString,
        agentName: NullableString,
        agentRank: Schema.NullOr(AgentRank),
        fleet: NullableString,
        ship: NullableString,
        fleetId: Schema.NullOr(Uuid),
        shipId: Schema.NullOr(Uuid),
        isAdmin: Schema.Boolean,
      }),
    }),
    HttpApiEndpoint.get("myInitiatives", "/me/initiatives", {
      success: Schema.Struct({ items: Schema.Array(MemberInitiative) }),
      error: endpointErrors,
    }),
  )
  .prefix(prefix)
  .middleware(BearerAuth);

const fleetParams = { reference: FleetRef };
const fleetGroup = HttpApiGroup.make("fleets")
  .add(
    HttpApiEndpoint.get("fleets", "/fleets", {
      query: PageQuery,
      success: pageOf(Fleet),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("fleet", "/fleets/:reference", {
      params: fleetParams,
      success: Fleet,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("registerFleet", "/fleets", {
      payload: CreateFleet,
      success: Fleet.pipe(HttpApiSchema.status(201)),
      error: endpointErrors,
    }),
    HttpApiEndpoint.patch("editFleet", "/fleets/:reference", {
      params: fleetParams,
      payload: UpdateFleet,
      success: Fleet,
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("listDecisionOutcomes", "/fleets/:fleetId/decision-outcomes", {
      params: { fleetId: Uuid },
      query: PageQuery,
      success: pageOf(DecisionOutcome),
      error: endpointErrors,
    }),
    HttpApiEndpoint.post(
      "acknowledgeDecisionOutcome",
      "/fleets/:fleetId/decision-outcomes/:id/delivered",
      {
        params: { fleetId: Uuid, id: Uuid },
        success: Schema.Void.pipe(HttpApiSchema.status(204)),
        error: endpointErrors,
      },
    ),
    HttpApiEndpoint.get("listShips", "/fleets/:fleetId/ships", {
      params: { fleetId: Uuid },
      success: Schema.Array(Ship),
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("createShip", "/fleets/:fleetId/ships", {
      params: { fleetId: Uuid },
      payload: CreateShip,
      success: Ship.pipe(HttpApiSchema.status(201)),
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      "Any human member may create a ship. Setting its channel requires a human organisation administrator, as the channel decides every member's autojoin; a member who supplies one receives 403.",
    ),
    HttpApiEndpoint.patch("renameShip", "/ships/:shipId", {
      params: { shipId: Uuid },
      payload: RenameShip,
      success: Ship,
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      "Any human member may rename a ship. Changing its channel requires a human organisation administrator; a member who supplies a different channel receives 403. Leaving channel out keeps it, and null clears it.",
    ),
  )
  .annotate(HttpApi.PayloadParseOptions, { onExcessProperty: "error" })
  .prefix(prefix)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

const initiativeParams = { id: Uuid };
const versionPayload = Schema.Struct({ version: Version });
const initiativeGroup = HttpApiGroup.make("initiatives")
  .add(
    HttpApiEndpoint.get("initiatives", "/initiatives", {
      query: InitiativeQuery,
      success: InitiativePage,
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("initiative", "/initiatives/:id", {
      params: initiativeParams,
      success: InitiativeDetail,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("createInitiative", "/initiatives", {
      payload: CreateInitiative,
      success: InitiativeDetail.pipe(HttpApiSchema.status(201)),
      error: endpointErrors,
    }),
    HttpApiEndpoint.patch("updateInitiative", "/initiatives/:id", {
      params: initiativeParams,
      payload: UpdateInitiative,
      success: InitiativeDetail,
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      "An agent registered to a fleet that changes parentId receives 403 when the initiative, or the new parent, belongs to another fleet. An initiative belongs to its own fleet, or else to the fleet of its nearest ancestor with one.",
    ),
    HttpApiEndpoint.post("archiveInitiative", "/initiatives/:id/archive", {
      params: initiativeParams,
      payload: versionPayload,
      success: InitiativeDetail,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("unarchiveInitiative", "/initiatives/:id/unarchive", {
      params: initiativeParams,
      payload: versionPayload,
      success: InitiativeDetail,
      error: endpointErrors,
    }),
    HttpApiEndpoint.patch("configureChannels", "/initiatives/:id/channel-configuration", {
      params: initiativeParams,
      payload: ChannelChanges,
      success: ChannelConfiguration,
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("initiativeProjects", "/initiatives/:id/projects", {
      params: initiativeParams,
      query: { ...PageQuery, scope: Schema.optional(Schema.Literals(["all", "direct"])) },
      success: Schema.Struct({
        items: Schema.Array(Project),
        nextCursor: NullableString,
        totalCount: Schema.Int,
        directCount: Schema.Int,
        groups: Schema.Array(
          Schema.Struct({
            id: Uuid,
            name: Name,
            status: InitiativeDetail.fields.status,
            archived: Schema.Boolean,
            ancestorArchived: Schema.Boolean,
            path: InitiativeDetail.fields.path,
          }),
        ),
      }),
      error: endpointErrors,
    }),
    HttpApiEndpoint.put("assignFleet", "/initiatives/:id/fleet", {
      params: initiativeParams,
      payload: FleetAssignment,
      success: InitiativeDetail,
      error: endpointErrors,
    }),
  )
  .prefix(prefix)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const DevCredentials = Schema.Struct({
  email: Schema.optional(Email),
  name: Schema.optional(Name),
  organisation: Schema.optional(Slug),
});

const devGroup = HttpApiGroup.make("development")
  .add(
    HttpApiEndpoint.post("devToken", "/auth/dev/token", {
      payload: DevCredentials,
      success: Schema.Struct({
        accessToken: Schema.String,
        tokenType: Schema.Literal("Bearer"),
        expiresIn: Schema.Int,
      }),
      error: endpointErrors,
    }).annotate(OpenApi.Description, "Available only when dev authentication is enabled."),
  )
  .prefix(prefix)
  .middleware(ValidationErrors);

const groups = [
  healthGroup,
  identityGroup,
  fleetGroup,
  initiativeGroup,
  stakeholderGroup,
  projectGroup,
  workflowGroup,
  planningGroup,
  cycleGroup,
  viewGroup,
  issueGroup,
  commentGroup,
  updatesGroup,
  mediaGroup,
  decisionGroup,
  documentGroup,
  reviewGroup,
  githubGroup,
] as const;

const api = HttpApi.make("subspace")
  .add(
    healthGroup,
    identityGroup,
    ircCredentialGroup,
    fleetGroup,
    initiativeGroup,
    stakeholderGroup,
    projectGroup,
    workflowGroup,
    planningGroup,
    cycleGroup,
    viewGroup,
    issueGroup,
    commentGroup,
    updatesGroup,
    mediaGroup,
    decisionGroup,
    documentGroup,
    reviewGroup,
    githubGroup,
    githubEventGroup,
    todoGroup,
    mcpGroup,
    agentGroup,
    agentCredentialGroup,
    enrolmentTokenGroup,
    enrolmentGroup,
    auditGroup,
    notificationsGroup,
  )
  .annotateMerge(OpenApi.annotations({ title: "Subspace API", version: "1.0.0" }));
export const documentedApi = api.add(devGroup);

/**
 * The agent listener's API: every group except development sign-in, the signed GitHub webhook and
 * IRC credentials. Its administrator routes are refused by `agentRouteFilter` before any handler
 * runs.
 */
export const agentApi = HttpApi.make("subspace").add(
  ...groups,
  todoGroup,
  mcpGroup,
  agentGroup,
  auditGroup,
  notificationsGroup,
);

const healthLive = HttpApiBuilder.group(api, "health", (handlers) =>
  handlers.handle("health", () =>
    Effect.flatMap(Database, (db) => db.execute(sql`SELECT 1`)).pipe(
      Effect.as({ status: "ok" as const }),
      Effect.mapError(() => problem(503, "The database is unavailable.")),
    ),
  ),
);

const identityLive = HttpApiBuilder.group(api, "identity", (handlers) =>
  handlers
    .handle("me", () => Effect.map(CurrentMember, publicProfile))
    .handle("myInitiatives", () => handleDb(Effect.flatMap(CurrentMember, memberInitiatives))),
);

const fleetLive = HttpApiBuilder.group(api, "fleets", (handlers) =>
  handlers
    .handle("fleets", ({ query }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => listFleets(member, query))),
    )
    .handle("fleet", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => getFleet(member, params.reference))),
    )
    .handle("registerFleet", ({ payload }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => createFleet(member, payload))),
    )
    .handle("editFleet", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => updateFleet(member, params.reference, payload)),
      ),
    )
    .handle("listDecisionOutcomes", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          listDecisionOutcomes(member, params.fleetId, query),
        ),
      ),
    )
    .handle("acknowledgeDecisionOutcome", ({ params }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          acknowledgeDecisionOutcome(member, params.fleetId, params.id),
        ),
      ),
    )
    .handle("listShips", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => listShips(member, params.fleetId))),
    )
    .handle("createShip", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          createShip(member, params.fleetId, payload.name, payload.channel),
        ),
      ),
    )
    .handle("renameShip", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => renameShip(member, params.shipId, payload)),
      ),
    ),
);

const initiativeLive = HttpApiBuilder.group(api, "initiatives", (handlers) =>
  handlers
    .handle("initiatives", ({ query }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => listInitiatives(member, query))),
    )
    .handle("initiative", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => getInitiative(member, params.id))),
    )
    .handle("createInitiative", ({ payload }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => createInitiative(member, payload))),
    )
    .handle("updateInitiative", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => updateInitiative(member, params.id, payload)),
      ),
    )
    .handle("archiveInitiative", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          updateInitiative(member, params.id, { ...payload, archived: true }),
        ),
      ),
    )
    .handle("unarchiveInitiative", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          updateInitiative(member, params.id, { ...payload, archived: false }),
        ),
      ),
    )
    .handle("configureChannels", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => updateChannels(member, params.id, payload)),
      ),
    )
    .handle("initiativeProjects", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => listInitiativeProjects(member, params.id, query)),
      ),
    )
    .handle("assignFleet", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => assignFleet(member, params.id, payload.fleetId)),
      ),
    ),
);

export const defaultDevEmail = "dev@example.com";
export const defaultDevName = "Local Developer";
export const defaultDevOrg = "local";

function localUrl(value: string) {
  try {
    return ["localhost", "127.0.0.1", "[::1]"].includes(new URL(value).hostname);
  } catch {
    return false;
  }
}

function devLive(dev: NonNullable<Config["dev"]>, key: Uint8Array, reserved: ReadonlySet<string>) {
  return HttpApiBuilder.group(documentedApi, "development", (handlers) =>
    handlers.handle("devToken", ({ payload, request }) =>
      handleDb(
        Effect.gen(function* () {
          const origin = request.headers.origin;
          if (origin && !localUrl(origin)) {
            return yield* Effect.fail(problem(403, "Dev sign-in requires a local browser origin."));
          }
          const email = (payload.email ?? defaultDevEmail).toLowerCase();
          const organisation = payload.organisation ?? defaultDevOrg;
          // Without a name, the username comes from the e-mail, not the default display name.
          const member = yield* localMember(
            email,
            payload.name ?? defaultDevName,
            organisation,
            dev,
            reserved,
            { name: payload.name, email },
          );
          if (!member.active)
            return yield* Effect.fail(problem(403, "This membership is inactive."));
          const accessToken = yield* Effect.promise(() => mintDevToken(key, email, organisation));
          return { accessToken, tokenType: "Bearer" as const, expiresIn: devTokenLifetime };
        }),
      ),
    ),
  );
}

// These resources share one problem document across API providers. Keep provider-specific detail.
const sharedProblems = (unauthorised: string) =>
  HttpRouter.middleware<Types.unhandled, HttpServerRequest.HttpServerRequest, true>(
    (next) =>
      Effect.gen(function* () {
        const request = yield* HttpServerRequest.HttpServerRequest;
        const response = yield* next;
        const path = request.url.split("?", 1)[0]!;
        if (!/^\/api\/v1\/(?:agents|fleets|ships|decisions)(?:\/|$)/u.test(path)) return response;
        const input = request.headers["x-correlation-id"];
        const correlationId =
          input && /^[a-zA-Z0-9._-]{1,128}$/u.test(input) ? input : crypto.randomUUID();
        const withCorrelation = HttpServerResponse.setHeader(
          response,
          "X-Correlation-Id",
          correlationId,
        );
        const body = response.body;
        if (body._tag !== "Uint8Array" || !body.contentType.startsWith("application/problem+json"))
          return withCorrelation;
        const { title, status, detail, currentVersion, errors } = JSON.parse(
          body.text ?? new TextDecoder().decode(body.body),
        );
        // The enrolment routes' own refusals say which enrolment token was refused, and why.
        const keepDetail = path === enrolmentPath || path === enrolmentCheckPath;
        return HttpServerResponse.jsonUnsafe(
          {
            type: `https://httpstatuses.com/${status}`,
            title,
            status,
            detail: status === 401 && !keepDetail ? unauthorised : detail,
            instance: path,
            correlationId,
            errors: errors ?? {},
            ...(currentVersion == null ? {} : { currentVersion }),
          },
          {
            status: response.status,
            headers: withCorrelation.headers,
            contentType: "application/problem+json",
          },
        );
      }),
    { global: true },
  );

function resourceLayers(options: Config, store: (typeof Storage)["Service"]) {
  const dbLayer = databaseLayer(options.databaseUrl, options.databasePoolSize);
  const github = options.github.app ? createGitHubClient(options.github.app) : null;
  const resources = Layer.mergeAll(
    dbLayer,
    updateHubLayer(options.updates).pipe(Layer.provide(dbLayer)),
    Layer.succeed(Storage, store),
    Layer.succeed(GithubIntegration, {
      secret: Redacted.make(options.github.webhookSecret),
      organisationId: options.github.organisationId,
      installationId: options.github.app?.installationId,
      client: github,
    }),
    Layer.succeed(GithubPollState, githubPollState(options.github)),
    Layer.succeed(ReservedUsernames, options.reservedUsernames),
  );
  return { dbLayer, github, resources };
}

export type Resources = ReturnType<typeof resourceLayers>;

/** The handlers of every group but development sign-in, built afresh for each listener. */
function handlerLayers(hiddenTools: ReadonlySet<string>) {
  return Layer.mergeAll(
    healthLive,
    identityLive,
    fleetLive,
    initiativeLive,
    HttpApiBuilder.group(api, "stakeholders", stakeholderHandlers),
    HttpApiBuilder.group(api, "projects", projectsHandlers),
    HttpApiBuilder.group(api, "workflow", workflowHandlers),
    HttpApiBuilder.group(api, "planning", planningHandlers),
    HttpApiBuilder.group(api, "cycles", cycleHandlers),
    HttpApiBuilder.group(api, "views", viewHandlers),
    HttpApiBuilder.group(api, "issues", issuesHandlers),
    HttpApiBuilder.group(api, "comments", commentHandlers),
    HttpApiBuilder.group(api, "updates", updatesHandlers),
    HttpApiBuilder.group(api, "media", mediaHandlers),
    HttpApiBuilder.group(api, "decisions", decisionHandlers),
    HttpApiBuilder.group(api, "documents", documentHandlers),
    HttpApiBuilder.group(api, "reviews", reviewHandlers),
    HttpApiBuilder.group(api, "github", githubHandlers),
    HttpApiBuilder.group(api, "githubWebhook", githubEventHandlers),
    HttpApiBuilder.group(api, "todos", todoHandlers),
    HttpApiBuilder.group(api, "mcp", mcpHandlersWithout(hiddenTools)),
    HttpApiBuilder.group(api, "agents", agentHandlers),
    HttpApiBuilder.group(api, "audit", auditHandlers),
    HttpApiBuilder.group(api, "notifications", notificationHandlers),
  );
}

export function routes(
  options: Config,
  devKey: Uint8Array | null,
  store: (typeof Storage)["Service"],
  shared: Resources = resourceLayers(options, store),
  fetchRemote?: RemoteFetch,
) {
  const { dbLayer, github, resources } = shared;
  const verify = jwtVerifiers(options, devKey, fetchRemote);
  const agentTokens: AgentTokenPolicy = {
    audience: options.agentTokenAudience,
    providers: new Set(
      (["workos", "entra", "dev"] as const).filter((provider) => options[provider] !== null),
    ),
  };
  const signIn: SignInPolicy = {
    autoRegister: options.autoRegister,
    entraAdminRole: options.entra?.adminRole ?? null,
    reservedUsernames: options.reservedUsernames,
    registerGuests: options.entra?.registerGuests ?? false,
  };
  const auth = Layer.effect(
    BearerAuth,
    Effect.gen(function* () {
      const db = yield* Database;
      return BearerAuth.of({
        bearer: (next, { credential }) =>
          Effect.gen(function* () {
            const request = yield* HttpServerRequest.HttpServerRequest;
            if (!/^Bearer [^\s]+$/i.test(request.headers.authorization ?? "")) {
              return yield* Effect.fail(problem(401, "A bearer access token is required."));
            }
            const token = Redacted.value(credential);
            // An agent token, signed with the agent's registered key, is only ever checked
            // against agent credentials, never by an identity provider's verifier.
            if (unverifiedIssuer(token) === agentTokenIssuer) {
              const agent = yield* verifyAgentToken(token, agentTokens).pipe(
                Effect.provideService(Database, db),
              );
              return yield* next.pipe(
                Effect.provideService(CurrentMember, {
                  ...agent.member,
                  tokenExpiresAt: agent.expiresAt,
                  agentCredential: agent.credential,
                }),
                Effect.map(noStore),
              );
            }
            const identity = yield* Effect.promise(() => verify(token));
            if (!identity)
              return yield* Effect.fail(
                problem(401, "The bearer access token is invalid or expired."),
              );
            const member = yield* signedInMember(identity, signIn).pipe(
              Effect.provideService(Database, db),
              Effect.tapError((error) =>
                Effect.logError("The membership could not be resolved", databaseFailure(error)),
              ),
              Effect.mapError(() => problem(500, "The membership could not be resolved.")),
            );
            if (member?.agentRank && !member.active)
              return yield* Effect.fail(
                problem(
                  403,
                  `Agent '${member.displayName}' is deactivated. Ask a human to reactivate it.`,
                ),
              );
            if (!member?.active)
              return yield* Effect.fail(
                problem(403, "An active organisation membership is required."),
              );
            return yield* next.pipe(
              Effect.provideService(CurrentMember, {
                ...member,
                tokenExpiresAt: identity.expiresAt,
              }),
              Effect.map(noStore),
            );
          }),
      });
    }),
  ).pipe(Layer.provide(dbLayer));

  // Groups the main listener alone serves: IRC credentials, agents' key credentials and fleet
  // enrolment.
  const common = Layer.mergeAll(
    handlerLayers(new Set()),
    HttpApiBuilder.group(
      api,
      "ircCredentials",
      ircCredentialHandlers(options.irc.credentialSecret, options.irc.servers),
    ),
    HttpApiBuilder.group(api, "agentCredentials", agentCredentialHandlers),
    HttpApiBuilder.group(api, "enrolmentTokens", enrolmentTokenHandlers),
    HttpApiBuilder.group(api, "enrolment", enrolmentHandlers),
  );
  const services = Layer.mergeAll(
    auth,
    validationErrors,
    enrolmentAuth.pipe(Layer.provide(dbLayer)),
  );

  const endpoints = (
    options.dev && devKey
      ? HttpApiBuilder.layer(documentedApi, { openapiPath: "/openapi.json" }).pipe(
          Layer.provide(
            Layer.merge(common, devLive(options.dev, devKey, options.reservedUsernames)),
          ),
          Layer.provide(services),
        )
      : HttpApiBuilder.layer(api, { openapiPath: "/openapi.json" }).pipe(
          Layer.provide(common),
          Layer.provide(services),
        )
  ).pipe(HttpRouter.provideRequest(resources));
  const cleanup = Layer.effectDiscard(
    purgeUploads.pipe(
      Effect.catch(() => Effect.logWarning("The upload cleanup could not complete.")),
      Effect.repeat(Schedule.spaced("1 minute")),
      Effect.forkScoped,
    ),
  ).pipe(Layer.provide(resources));
  // Bounds the record of accepted agent tokens: each row outlives its token by a few minutes.
  const tokenCleanup = Layer.effectDiscard(
    purgeAgentTokenIds.pipe(
      Effect.catch(() => Effect.logWarning("The agent token cleanup could not complete.")),
      Effect.repeat(Schedule.spaced("1 minute")),
      Effect.forkScoped,
    ),
  ).pipe(Layer.provide(resources));
  const polling = Layer.effectDiscard(pollGitHub(options.github, github)).pipe(
    Layer.provide(resources),
  );
  // Ergo's auth hook: outside the HttpApi, so outside the OpenAPI document and the bearer
  // middleware, and on this listener only.
  const ircHook = ircAuthenticateLayer(options.irc).pipe(HttpRouter.provideRequest(resources));
  return Layer.mergeAll(
    endpoints,
    ircHook,
    cleanup,
    tokenCleanup,
    polling,
    sharedProblems("A valid Subspace access token is required."),
  );
}

/** The agent listener's handlers: every group's, without the administrator endpoints or tools. */
export const agentHandlerLayers = () =>
  withoutAdminEndpoints(handlerLayers(adminOnlyTools), agentApi.groups);

/**
 * The agent listener's routes: the same handlers and rules as the main listener, with the agent
 * named in a header instead of a bearer token, no sign-in route, no administrator route or MCP
 * tool, and no background work of its own.
 */
export function agentRoutes(
  listener: AgentListener,
  shared: Pick<Resources, "dbLayer" | "resources">,
  directory: Layer.Layer<AgentDirectory, unknown> = agentDirectory(listener).pipe(
    Layer.provide(shared.dbLayer),
  ),
) {
  const auth = agentAuth.pipe(Layer.provide(directory));
  return Layer.mergeAll(
    HttpApiBuilder.layer(agentApi).pipe(
      Layer.provide(agentHandlerLayers()),
      Layer.provide(Layer.merge(auth, validationErrors)),
      HttpRouter.provideRequest(shared.resources),
    ),
    agentRequestGuard,
    sharedProblems(`Identify the agent with the X-Subspace-Agent header on this listener.`),
  );
}

/**
 * What a test may replace in `server`: the shared resources, the agent directory and the fetch
 * that reaches identity providers' discovery documents and keys.
 */
export interface ServerOverrides {
  readonly store?: (typeof Storage)["Service"];
  readonly shared?: Resources;
  readonly directory?: Layer.Layer<AgentDirectory, unknown>;
  readonly fetchRemote?: RemoteFetch;
}

export function server(
  options: Config,
  devKey: Uint8Array | null,
  overrides: ServerOverrides = {},
) {
  const store = overrides.store ?? storage(process.env);
  const shared = overrides.shared ?? resourceLayers(options, store);
  const main = HttpRouter.serve(routes(options, devKey, store, shared, overrides.fetchRemote), {
    disableLogger: true,
  }).pipe(
    Layer.provide(
      BunHttpServer.layer({
        hostname: options.hostname,
        port: options.port,
        maxRequestBodySize: 34_000_000,
      }),
    ),
  );
  if (!options.agent) return main.pipe(Layer.provide(shared.resources));
  const agent = HttpRouter.serve(agentRoutes(options.agent, shared, overrides.directory), {
    disableLogger: true,
  }).pipe(
    Layer.provide(
      BunHttpServer.layer({
        hostname: options.agent.hostname,
        port: options.agent.port,
        maxRequestBodySize: 34_000_000,
      }),
    ),
  );
  // Build the database pool, update hub and storage once, before either listener, so both
  // listeners share them rather than each building its own. Each HttpRouter.serve builds its app
  // with a forked MemoMap, so its HttpRouter and routes are private to that listener ("Layers
  // first built inside the app are private to this server"): the agent port never serves the
  // main port's sign-in or administrator routes, and the agent guard never runs on the main
  // port. tests/listeners.test.ts builds this composition and checks both ports.
  return Layer.merge(main, agent).pipe(Layer.provide(shared.resources));
}
