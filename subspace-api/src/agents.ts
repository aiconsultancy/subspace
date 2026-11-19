import { and, eq, isNotNull, sql } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { HttpApiEndpoint, HttpApiGroup, HttpApiSchema } from "effect/http-api";
import { recordAudit } from "./audit";
import { Database } from "./db";
import { AgentRank, CurrentMember, type Member } from "./identity";
import { Version } from "./initiative-contract";
import { BearerAuth, handleDb, ValidationErrors, versionedApi } from "./middleware";
import { byKey, page } from "./paging";
import { endpointErrors, problem } from "./problems";
import { organisations, users } from "./schema";
import {
  dateTime,
  dateTimeOrNull,
  Line,
  Name,
  NullableString,
  PageQuery,
  pageOf,
  Timestamp,
  Uuid,
} from "./validation";

const Registration = Schema.Struct({
  name: Name,
  rank: AgentRank,
  fleet: Name,
  ship: Schema.optional(Schema.NullOr(Name)),
  defaultModel: Line(120),
  serviceTokenClientId: Line(256),
});
const Edit = Schema.Struct({ ...Registration.fields, version: Schema.optional(Version) });
const Lifecycle = Schema.Struct({ version: Schema.optional(Version) });
const Agent = Schema.Struct({
  userId: Uuid,
  name: Name,
  rank: AgentRank,
  fleet: Name,
  ship: NullableString,
  defaultModel: Line(120),
  serviceTokenClientId: Line(256),
  isActive: Schema.Boolean,
  deactivatedAt: Schema.NullOr(Timestamp),
  deactivatedByUserId: Schema.NullOr(Uuid),
  registeredByUserId: Uuid,
  version: Version,
  createdAt: Timestamp,
  updatedAt: Timestamp,
});

function dto(agent: Member) {
  return {
    userId: agent.id,
    name: agent.displayName,
    rank: agent.agentRank!,
    fleet: agent.agentFleet!,
    ship: agent.agentShip,
    defaultModel: agent.agentModel!,
    serviceTokenClientId: agent.subject,
    isActive: agent.active,
    deactivatedAt: dateTimeOrNull(agent.deactivatedAt),
    deactivatedByUserId: agent.deactivatedById,
    registeredByUserId: agent.registeredById!,
    version: agent.version,
    createdAt: dateTime(agent.createdAt),
    updatedAt: dateTime(agent.updatedAt),
  };
}

const scope = (member: Member, id?: string) =>
  and(
    eq(users.organisationId, member.organisationId),
    isNotNull(users.agentRank),
    id ? eq(users.id, id) : undefined,
  );

function listAgents(member: Member, query: { limit?: number; cursor?: string }) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const rows = yield* db.select().from(users).where(scope(member));
    const key = (row: Member) => `${row.displayName.toLowerCase()}:${row.id}`;
    const result = yield* page(
      rows.sort(byKey(key)),
      { agents: member.organisationId },
      key,
      query.limit,
      query.cursor,
    );
    return { ...result, items: result.items.map(dto) };
  });
}

function getAgent(member: Member, id: string) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const [agent] = yield* db.select().from(users).where(scope(member, id));
    return agent ? dto(agent) : yield* Effect.fail(problem(404, "Agent not found."));
  });
}

function registrationFields(member: Member, data: typeof Registration.Type) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const [org] = yield* db
      .select({ provider: organisations.provider })
      .from(organisations)
      .where(eq(organisations.id, member.organisationId));
    if (data.rank === "captain" && !data.ship)
      return yield* Effect.fail(problem(400, "A captain requires a ship."));
    const subject =
      org?.provider === "workos"
        ? data.serviceTokenClientId
        : data.serviceTokenClientId.toLowerCase();
    if (
      (org?.provider === "entra" && !Schema.is(Uuid)(subject)) ||
      (org?.provider === "dev" && !/^[^\s@]+@[^\s@]+\.[^\s@]+$/u.test(subject))
    )
      return yield* Effect.fail(
        problem(400, "Use the account subject format for this identity provider."),
      );
    return {
      displayName: data.name,
      agentRank: data.rank,
      agentFleet: data.fleet,
      agentShip: data.ship ?? null,
      agentModel: data.defaultModel,
      subject,
    };
  });
}

type AgentChange =
  | { kind: "register" | "update"; registration: typeof Registration.Type; version?: number }
  | { kind: "activate" | "deactivate"; version?: number };

function changeAgent(member: Member, change: AgentChange, id?: string) {
  return Effect.gen(function* () {
    const db = yield* Database;
    if (member.role !== "admin" || member.agentRank)
      return yield* Effect.fail(
        problem(403, "Agent registration changes require an organisation administrator."),
      );
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        yield* tx
          .select({ id: organisations.id })
          .from(organisations)
          .where(eq(organisations.id, member.organisationId))
          .for("update");
        const [old] = id ? yield* tx.select().from(users).where(scope(member, id)) : [];
        if (id && !old) return yield* Effect.fail(problem(404, "Agent not found."));
        if (old && change.version !== undefined && change.version !== old.version)
          return yield* Effect.fail(problem(409, "The agent version is stale.", old.version));
        const active =
          change.kind === "activate" ? true : change.kind === "deactivate" ? false : undefined;
        if (old && active === old.active) return dto(old);
        const registration =
          "registration" in change
            ? yield* registrationFields(member, change.registration).pipe(
                Effect.provideService(Database, tx),
              )
            : undefined;
        if (registration && old && registration.subject !== old.subject)
          return yield* Effect.fail(
            problem(409, "Register a new agent to bind a different account subject."),
          );
        if (registration) {
          const [conflict] = yield* tx
            .select({ id: users.id })
            .from(users)
            .where(
              and(
                eq(users.organisationId, member.organisationId),
                sql`(${users.subject} = ${registration.subject} or
            (${users.agentRank} is not null and lower(${users.displayName}) = lower(${registration.displayName})))`,
                id ? sql`${users.id} <> ${id}` : undefined,
              ),
            );
          if (conflict)
            return yield* Effect.fail(
              problem(409, "An account subject or agent name is already registered."),
            );
        }
        const [saved] = old
          ? yield* tx
              .update(users)
              .set({
                ...registration,
                active,
                deactivatedAt: active === undefined ? undefined : active ? null : new Date(),
                deactivatedById: active === undefined ? undefined : active ? null : member.id,
                version: old.version + 1,
                updatedAt: sql`now()`,
              })
              .where(scope(member, old.id))
              .returning()
          : yield* tx
              .insert(users)
              .values({
                ...registration!,
                organisationId: member.organisationId,
                registeredById: member.id,
              })
              .onConflictDoNothing()
              .returning();
        if (!saved) return yield* Effect.fail(problem(409, "The agent registration changed."));
        const action = {
          register: "agentRegistered",
          update: "agentUpdated",
          activate: "agentReactivated",
          deactivate: "agentDeactivated",
        }[change.kind];
        yield* recordAudit(tx, member, {
          action,
          entityType: "Agent",
          entityId: saved.id,
          summary: "Changed an agent registration.",
          changes: { rank: saved.agentRank, active: saved.active },
        });
        return dto(saved);
      }),
    );
  });
}

const params = { agentId: Uuid };
export const agentGroup = HttpApiGroup.make("agents")
  .add(
    HttpApiEndpoint.get("listAgents", "/agents", {
      query: PageQuery,
      success: pageOf(Agent),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("getAgent", "/agents/:agentId", {
      params,
      success: Agent,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("registerAgent", "/agents", {
      payload: Registration,
      success: Agent.pipe(HttpApiSchema.status(201)),
      error: endpointErrors,
    }),
    HttpApiEndpoint.put("editAgent", "/agents/:agentId", {
      params,
      payload: Edit,
      success: Agent,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("deactivateAgent", "/agents/:agentId/deactivate", {
      params,
      payload: Lifecycle,
      success: Agent,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("reactivateAgent", "/agents/:agentId/reactivate", {
      params,
      payload: Lifecycle,
      success: Agent,
      error: endpointErrors,
    }),
  )
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const agentHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof agentGroup>,
) =>
  handlers
    .handle("listAgents", ({ query }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => listAgents(member, query))),
    )
    .handle("getAgent", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => getAgent(member, params.agentId))),
    )
    .handle("registerAgent", ({ payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          changeAgent(member, { kind: "register", registration: payload }),
        ),
      ),
    )
    .handle("editAgent", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          changeAgent(
            member,
            { kind: "update", registration: payload, version: payload.version },
            params.agentId,
          ),
        ),
      ),
    )
    .handle("deactivateAgent", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          changeAgent(member, { kind: "deactivate", version: payload.version }, params.agentId),
        ),
      ),
    )
    .handle("reactivateAgent", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          changeAgent(member, { kind: "activate", version: payload.version }, params.agentId),
        ),
      ),
    );
