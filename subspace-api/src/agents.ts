import { and, desc, eq, isNotNull, sql } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { HttpApiEndpoint, HttpApiGroup, HttpApiSchema } from "effect/http-api";
import { recordAudit } from "./audit";
import { Database } from "./db";
import { ShipName } from "./fleets";
import { AgentRank, CurrentMember, isKeyOnlySubject, type Member } from "./identity";
import { Version } from "./initiative-contract";
import { BearerAuth, handleDb, ValidationErrors, versionedApi } from "./middleware";
import { cursorTime, encodeCursor, readCursor } from "./paging";
import { endpointErrors, problem, validationProblem } from "./problems";
import { fleets, organisations, ships, users } from "./schema";
import {
  canonicalName,
  isReservedUsername,
  ReservedUsernames,
  retiredByAnother,
  usernameKey,
} from "./usernames";
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

/**
 * The agent's IRC nick, which fleet IRC servers tie to the account of the
 * same name: a letter, then letters, digits, `-` or `_`, at most 32 in all,
 * the account names the IRC server's sign-in script accepts
 * (`subspace-irc/subspace-auth.sh`). Checked when a nick is set; a longer nick
 * saved before this rule stays as it is until it is changed.
 */
export const ircNickPattern = /^[A-Za-z][A-Za-z0-9_-]{0,31}$/u;
export const IrcNick = Schema.String.check(
  Schema.isMaxLength(32),
  Schema.isPattern(ircNickPattern),
);
/** An agent's name: letters, digits and `-`, at most 64 characters, unique in its organisation. */
export const AgentName = Name.check(Schema.isMaxLength(64), Schema.isPattern(/^[A-Za-z0-9-]+$/u));
const Registration = Schema.Struct({
  name: AgentName,
  rank: AgentRank,
  fleet: Schema.optional(Schema.NullOr(Schema.String)),
  ship: Schema.optional(Schema.NullOr(Schema.String)),
  fleetId: Schema.optional(Schema.NullOr(Uuid)),
  shipId: Schema.optional(Schema.NullOr(Uuid)),
  defaultModel: Line(100),
  serviceTokenClientId: Line(200),
  ircNick: Schema.optional(Schema.NullOr(IrcNick)),
});
const Edit = Schema.Struct({ ...Registration.fields, version: Schema.optional(Version) });
const Lifecycle = Schema.Struct({ version: Schema.optional(Version) });
const Agent = Schema.Struct({
  userId: Uuid,
  name: Name,
  rank: AgentRank,
  fleet: Schema.String,
  ship: NullableString,
  fleetId: Schema.NullOr(Uuid),
  shipId: Schema.NullOr(Uuid),
  defaultModel: Line(120),
  serviceTokenClientId: Line(256),
  ircNick: Schema.NullOr(Schema.String),
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
    fleetId: agent.agentFleetId,
    shipId: agent.agentShipId,
    defaultModel: agent.agentModel!,
    serviceTokenClientId: agent.subject,
    ircNick: agent.agentIrcNick,
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

function agentRows(db: (typeof Database)["Service"], member: Member, id?: string) {
  return db
    .select({
      agent: users,
      fleetSlug: fleets.slug,
      shipName: ships.name,
      cursorTime: cursorTime(users.createdAt),
    })
    .from(users)
    .leftJoin(fleets, eq(users.agentFleetId, fleets.id))
    .leftJoin(ships, eq(users.agentShipId, ships.id))
    .where(scope(member, id))
    .$dynamic();
}

type LinkedRow = Effect.Success<ReturnType<typeof agentRows>>[number];

function linkedDto({ agent, fleetSlug, shipName }: LinkedRow) {
  return dto({
    ...agent,
    agentFleet: fleetSlug ?? agent.agentFleet,
    agentShip: shipName ?? agent.agentShip,
  });
}

function listAgents(member: Member, query: { limit?: number; cursor?: string }) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const { limit = 50, cursor } = query;
    const anchor = yield* readCursor(cursor);
    const rows = yield* agentRows(db, member)
      .where(
        and(
          scope(member),
          anchor
            ? sql`(${users.createdAt}, ${users.id}) < (${anchor.time}::timestamptz, ${anchor.id}::uuid)`
            : undefined,
        ),
      )
      .orderBy(desc(users.createdAt), desc(users.id))
      .limit(limit + 1);
    const selected = rows.slice(0, limit);
    const last = selected.at(-1);
    return {
      items: selected.map(linkedDto),
      nextCursor: last && rows.length > limit ? encodeCursor(last.cursorTime, last.agent.id) : null,
    };
  });
}

function getAgent(member: Member, id: string) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const [row] = yield* agentRows(db, member, id);
    return row
      ? linkedDto(row)
      : yield* Effect.fail(
          problem(404, "Agent was not found. Use the userId returned by the agents list."),
        );
  });
}

function registrationFields(member: Member, data: typeof Registration.Type, old?: Member) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const [org] = yield* db
      .select({ provider: organisations.provider })
      .from(organisations)
      .where(eq(organisations.id, member.organisationId));
    const subject =
      org?.provider === "workos"
        ? data.serviceTokenClientId
        : data.serviceTokenClientId.toLowerCase();
    // An enrolled agent's subject names no provider account: its edit may keep it, but no
    // registration may give such a subject.
    const keepsKeyOnly = isKeyOnlySubject(subject) && subject === old?.subject;
    if (!keepsKeyOnly && isKeyOnlySubject(subject))
      return yield* Effect.fail(
        validationProblem(
          "serviceTokenClientId",
          "This form of subject is kept for agents enrolled with a key. Use the agent's account subject.",
        ),
      );
    if (
      !keepsKeyOnly &&
      ((org?.provider === "entra" && !Schema.is(Uuid)(subject)) ||
        (org?.provider === "dev" && !/^[^\s@]+@[^\s@]+\.[^\s@]+$/u.test(subject)))
    )
      return yield* Effect.fail(
        problem(400, "Use the account subject format for this identity provider."),
      );
    const fleetSlug = data.fleet?.trim().toLowerCase();
    const shipName = data.ship?.trim() || null;
    const existingFleet =
      old?.agentFleetId === null && old.agentFleet === data.fleet ? old.agentFleet : null;
    if (
      !data.fleetId &&
      !existingFleet &&
      (!fleetSlug || !/^[a-z0-9]+(?:-[a-z0-9]+)*$/u.test(fleetSlug) || fleetSlug.length > 80)
    )
      return yield* Effect.fail(
        validationProblem("fleet", "Use a lower-case fleet slug of 1 to 80 characters."),
      );
    if (data.rank === "captain" && !data.shipId && !shipName)
      return yield* Effect.fail(validationProblem("ship", "A captain needs a ship."));
    if (data.rank === "rearAdmiral" && (data.shipId || shipName))
      return yield* Effect.fail(
        validationProblem("ship", "A rear admiral must not supply a ship."),
      );
    if (!data.shipId && shipName && !Schema.is(ShipName)(shipName))
      return yield* Effect.fail(
        validationProblem("ship", "Use a ship name on one line without control characters."),
      );

    const [fleet] = yield* db
      .select()
      .from(fleets)
      .where(
        and(
          eq(fleets.organisationId, member.organisationId),
          data.fleetId ? eq(fleets.id, data.fleetId) : eq(fleets.slug, fleetSlug!),
        ),
      )
      .for("share");
    if (!fleet && data.fleetId)
      return yield* Effect.fail(
        validationProblem("fleetId", "Fleet not found. Use an existing fleet ID."),
      );
    let ship: typeof ships.$inferSelect | undefined;
    if (data.shipId) {
      if (fleet)
        [ship] = yield* db
          .select()
          .from(ships)
          .where(and(eq(ships.fleetId, fleet.id), eq(ships.id, data.shipId)))
          .for("share");
      if (!ship)
        return yield* Effect.fail(
          validationProblem("shipId", "Choose a ship in the selected fleet."),
        );
    } else if (fleet && shipName) {
      yield* db
        .insert(ships)
        .values({ organisationId: member.organisationId, fleetId: fleet.id, name: shipName })
        .onConflictDoNothing();
      [ship] = yield* db
        .select()
        .from(ships)
        .where(and(eq(ships.fleetId, fleet.id), sql`lower(${ships.name}) = lower(${shipName})`))
        .for("share");
      if (!ship)
        return yield* Effect.fail(
          problem(409, "The ship name changed. Reload the fleet and retry."),
        );
    }
    return {
      displayName: data.name,
      agentRank: data.rank,
      agentFleet: fleet?.slug ?? existingFleet ?? fleetSlug!,
      agentShip: ship?.name ?? shipName,
      agentFleetId: fleet?.id ?? null,
      agentShipId: ship?.id ?? null,
      agentModel: data.defaultModel,
      // Left out of an edit, the nick stays as it is; null clears it. The username is the nick,
      // in the same write, so people and agents share one namespace.
      agentIrcNick: data.ircNick,
      username: data.ircNick,
      subject,
    };
  });
}

/**
 * Refuses an agent registration whose new IRC nick is reserved (400) or was retired by another
 * record (409), or whose account subject, agent name or IRC nick another record holds (409). A
 * person's username counts as a taken IRC nick: it is their IRC account name. Call it inside the
 * transaction that holds the organisation's lock, with the record being edited as `old`.
 */
export const checkAgentNames = (
  tx: (typeof Database)["Service"],
  organisationId: string,
  registration: { subject: string; displayName: string; agentIrcNick?: string | null },
  old?: { id: string; agentIrcNick: string | null },
) =>
  Effect.gen(function* () {
    const nick = registration.agentIrcNick;
    const newNick =
      typeof nick === "string" &&
      canonicalName(nick) !== (old?.agentIrcNick == null ? null : canonicalName(old.agentIrcNick));
    if (newNick && isReservedUsername(nick, yield* ReservedUsernames))
      return yield* Effect.fail(
        validationProblem("ircNick", "This IRC nick is reserved. Choose another."),
      );
    if (newNick && (yield* retiredByAnother(tx, organisationId, nick, old?.id)))
      return yield* Effect.fail(
        problem(
          409,
          "This IRC nick was used before in this organisation and cannot be given to another account.",
        ),
      );
    const nickKey = typeof nick === "string" ? canonicalName(nick) : null;
    const [conflict] = yield* tx
      .select({ id: users.id })
      .from(users)
      .where(
        and(
          eq(users.organisationId, organisationId),
          sql`(${users.subject} = ${registration.subject} or
            (${users.agentRank} is not null and lower(${users.displayName}) = lower(${registration.displayName})) or
            (${nickKey}::text is not null and
              (${usernameKey(users.agentIrcNick)} = ${nickKey}::text or
               ${usernameKey(users.username)} = ${nickKey}::text)))`,
          old ? sql`${users.id} <> ${old.id}` : undefined,
        ),
      );
    if (conflict)
      return yield* Effect.fail(
        problem(409, "An account subject, agent name or IRC nick is already registered."),
      );
  });

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
        yield* (
          tx
            .select({ id: organisations.id })
            .from(organisations)
            .where(eq(organisations.id, member.organisationId))
            // Serialize agent changes while allowing foreign keys to take a key-share lock.
            .for("no key update")
        );
        const [old] = id ? yield* tx.select().from(users).where(scope(member, id)) : [];
        if (id && !old)
          return yield* Effect.fail(
            problem(404, "Agent was not found. Use the userId returned by the agents list."),
          );
        if (old && change.version !== undefined && change.version !== old.version)
          return yield* Effect.fail(
            problem(
              409,
              `The agent changed. Reload it and retry. Current version is ${old.version}.`,
              old.version,
            ),
          );
        const active =
          change.kind === "activate" ? true : change.kind === "deactivate" ? false : undefined;
        if (old && active === old.active)
          return yield* getAgent(member, old.id).pipe(Effect.provideService(Database, tx));
        const registration =
          "registration" in change
            ? yield* registrationFields(member, change.registration, old).pipe(
                Effect.provideService(Database, tx),
              )
            : undefined;
        if (registration && old && registration.subject !== old.subject)
          return yield* Effect.fail(
            problem(409, "Register a new agent to bind a different account subject."),
          );
        if (registration) yield* checkAgentNames(tx, member.organisationId, registration, old);
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
        return yield* getAgent(member, saved.id).pipe(Effect.provideService(Database, tx));
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
