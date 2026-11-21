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
import { agentGroup, agentHandlers } from "./agents";
import { auditGroup, auditHandlers } from "./audit";
import { devTokenLifetime, jwtVerifiers, mintDevToken } from "./auth";
import { commentGroup, commentHandlers } from "./comment-mutations";
import { type Config } from "./config";
import { documentGroup, documentHandlers } from "./documents";
import { reviewGroup, reviewHandlers } from "./reviews";
import { cycleGroup, cycleHandlers } from "./cycles";
import { decisionGroup, decisionHandlers } from "./decisions";
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
import { AgentRank, CurrentMember, findMember, localMember, publicProfile } from "./identity";
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
  handleDb,
  validationErrors,
  ValidationErrors,
  versionedApi as prefix,
} from "./middleware";
import { planningGroup, planningHandlers } from "./planning";
import { Project } from "./projects";
import { issueGroup, issuesHandlers, projectGroup, projectsHandlers } from "./issues-http";

import { mcpGroup, mcpHandlers } from "./mcp";
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
    }),
    HttpApiEndpoint.patch("renameShip", "/ships/:shipId", {
      params: { shipId: Uuid },
      payload: RenameShip,
      success: Ship,
      error: endpointErrors,
    }),
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

const api = HttpApi.make("subspace")
  .add(
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
    githubEventGroup,
    todoGroup,
    mcpGroup,
    agentGroup,
    auditGroup,
    notificationsGroup,
  )
  .annotateMerge(OpenApi.annotations({ title: "Subspace API", version: "1.0.0" }));
export const documentedApi = api.add(devGroup);

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
    .handle("listShips", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => listShips(member, params.fleetId))),
    )
    .handle("createShip", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => createShip(member, params.fleetId, payload.name)),
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

function devLive(dev: NonNullable<Config["dev"]>, key: Uint8Array) {
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
          const member = yield* localMember(
            email,
            payload.name ?? defaultDevName,
            organisation,
            dev,
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
const sharedProblems = HttpRouter.middleware<
  Types.unhandled,
  HttpServerRequest.HttpServerRequest,
  true
>(
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
      return HttpServerResponse.jsonUnsafe(
        {
          type: `https://httpstatuses.com/${status}`,
          title,
          status,
          detail: status === 401 ? "A valid Subspace access token is required." : detail,
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

export function routes(
  options: Config,
  devKey: Uint8Array | null,
  store: (typeof Storage)["Service"],
) {
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
  );
  const verify = jwtVerifiers(options, devKey);
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
            const identity = yield* Effect.promise(() => verify(Redacted.value(credential)));
            if (!identity)
              return yield* Effect.fail(
                problem(401, "The bearer access token is invalid or expired."),
              );
            const member = yield* findMember(identity).pipe(
              Effect.provideService(Database, db),
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

  const common = Layer.mergeAll(
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
    HttpApiBuilder.group(api, "mcp", mcpHandlers),
    HttpApiBuilder.group(api, "agents", agentHandlers),
    HttpApiBuilder.group(api, "audit", auditHandlers),
    HttpApiBuilder.group(api, "notifications", notificationHandlers),
  );
  const services = Layer.merge(auth, validationErrors);

  const endpoints = (
    options.dev && devKey
      ? HttpApiBuilder.layer(documentedApi, { openapiPath: "/openapi.json" }).pipe(
          Layer.provide(Layer.merge(common, devLive(options.dev, devKey))),
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
  const polling = Layer.effectDiscard(pollGitHub(options.github, github)).pipe(
    Layer.provide(resources),
  );
  return Layer.mergeAll(endpoints, cleanup, polling, sharedProblems);
}

export function server(options: Config, devKey: Uint8Array | null) {
  return HttpRouter.serve(routes(options, devKey, storage(process.env)), {
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
}
