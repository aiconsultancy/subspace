import { and, desc, eq, inArray, or, sql } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { Database } from "./db";
import { AgentRank, type Member } from "./identity";
import { cursorTime, encodeCursor, readCursor } from "./paging";
import { problem, validationProblem } from "./problems";
import { fleets, ships, users } from "./schema";
import { dateTime, Host, isUuid, Line, Name, Port, Slug, Timestamp, Uuid } from "./validation";

const Version = Schema.Int.check(Schema.isGreaterThan(0));

type Db = (typeof Database)["Service"];
type FleetRow = typeof fleets.$inferSelect;
type ShipRow = typeof ships.$inferSelect;

const Websocket = Schema.String.check(
  Schema.isMaxLength(2048),
  Schema.isPattern(/^wss?:\/\/[^\s/@?#]+(?:[/?#][^\s]*)?$/u),
);
const editable = {
  slug: Slug.check(Schema.makeFilter((slug) => !isUuid(slug), { expected: "a slug, not a UUID" })),
  name: Name,
  ircHost: Host,
  ircPort: Port,
  ircTls: Schema.Boolean,
  ircWebsocketUrl: Schema.optional(Schema.NullOr(Websocket)),
  vmHost: Schema.optional(Schema.NullOr(Host)),
};
export const CreateFleet = Schema.Struct(editable).annotate({
  parseOptions: { onExcessProperty: "error" },
});
export const UpdateFleet = Schema.Struct({
  slug: Schema.optional(editable.slug),
  name: Schema.optional(Name),
  ircHost: Schema.optional(Host),
  ircPort: Schema.optional(Port),
  ircTls: Schema.optional(Schema.Boolean),
  ircWebsocketUrl: editable.ircWebsocketUrl,
  vmHost: editable.vmHost,
  version: Schema.optional(Version),
});
export const ShipName = Line(64);
export const CreateShip = Schema.Struct({ name: ShipName });
export const RenameShip = Schema.Struct({ name: ShipName, version: Schema.optional(Version) });
export const Ship = Schema.Struct({
  id: Uuid,
  fleetId: Uuid,
  name: Schema.String,
  createdAt: Timestamp,
  updatedAt: Timestamp,
  version: Version,
}).annotate({ identifier: "Ship" });
const FleetAgent = Schema.Struct({
  userId: Uuid,
  name: Schema.String,
  rank: AgentRank,
  shipId: Schema.NullOr(Uuid),
  ship: Schema.NullOr(Schema.String),
  available: Schema.Boolean,
});
export const Fleet = Schema.Struct({
  id: Uuid,
  slug: Schema.String,
  name: Schema.String,
  ircHost: Schema.NullOr(Host),
  ircPort: Schema.NullOr(Port),
  ircTls: Schema.NullOr(Schema.Boolean),
  ircWebsocketUrl: Schema.NullOr(Websocket),
  vmHost: Schema.NullOr(Host),
  rearAdmiralName: Schema.NullOr(Schema.String),
  captainNames: Schema.Array(Schema.String),
  irc: Schema.Struct({
    host: Schema.NullOr(Host),
    port: Schema.NullOr(Port),
    tls: Schema.NullOr(Schema.Boolean),
    websocketUrl: Schema.NullOr(Websocket),
  }),
  createdAt: Timestamp,
  updatedAt: Timestamp,
  version: Version,
  agents: Schema.Array(FleetAgent),
  ships: Schema.Array(Schema.Struct({ id: Uuid, name: Schema.String })),
}).annotate({ identifier: "Fleet" });
export const FleetRef = Schema.String.check(
  Schema.isMinLength(1),
  Schema.isMaxLength(80),
  Schema.isPattern(/^[^\0]*$/u),
);
export const FleetAssignment = Schema.Struct({ fleetId: Schema.NullOr(Uuid) });

const ranks = { rearAdmiral: 0, captain: 1, crew: 2 };
const compare = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);
const byName = (a: { name: string }, b: { name: string }) =>
  compare(a.name.toUpperCase(), b.name.toUpperCase());
const byShip = (a: ShipRow, b: ShipRow) => byName(a, b) || compare(a.id, b.id);

export const shipDto = ({ id, fleetId, name, createdAt, updatedAt, version }: ShipRow) => ({
  id,
  fleetId,
  name,
  createdAt: dateTime(createdAt),
  updatedAt: dateTime(updatedAt),
  version,
});

export function loadFleetDtos(db: Db, organisationId: string, rows: FleetRow[]) {
  return Effect.gen(function* () {
    const ids = rows.map(({ id }) => id);
    if (!ids.length) return new Map<string, typeof Fleet.Type>();
    const shipRows = yield* db
      .select()
      .from(ships)
      .where(and(eq(ships.organisationId, organisationId), inArray(ships.fleetId, ids)));
    const agents = yield* db
      .select()
      .from(users)
      .where(and(eq(users.organisationId, organisationId), inArray(users.agentFleetId, ids)));
    const shipsByFleet = Map.groupBy(shipRows.sort(byShip), ({ fleetId }) => fleetId);
    const shipsById = new Map(shipRows.map((ship) => [ship.id, ship.name]));
    const agentsByFleet = Map.groupBy(agents, ({ agentFleetId }) => agentFleetId);
    return new Map(
      rows.map((row) => {
        const roster = (agentsByFleet.get(row.id) ?? []).flatMap((agent) =>
          agent.agentRank
            ? [
                {
                  userId: agent.id,
                  name: agent.displayName,
                  rank: agent.agentRank,
                  shipId: agent.agentShipId,
                  ship:
                    (agent.agentShipId ? shipsById.get(agent.agentShipId) : undefined) ??
                    agent.agentShip,
                  available: agent.active,
                },
              ]
            : [],
        );
        roster.sort(
          (a, b) => ranks[a.rank] - ranks[b.rank] || byName(a, b) || compare(a.userId, b.userId),
        );
        return [
          row.id,
          {
            id: row.id,
            slug: row.slug,
            name: row.name,
            ircHost: row.ircHost,
            ircPort: row.ircPort,
            ircTls: row.ircTls,
            ircWebsocketUrl: row.ircWebsocketUrl,
            vmHost: row.vmHost,
            rearAdmiralName: roster.find(({ rank }) => rank === "rearAdmiral")?.name ?? null,
            captainNames: roster.filter(({ rank }) => rank === "captain").map(({ name }) => name),
            irc: {
              host: row.ircHost,
              port: row.ircPort,
              tls: row.ircTls,
              websocketUrl: row.ircWebsocketUrl,
            },
            createdAt: dateTime(row.createdAt),
            updatedAt: dateTime(row.updatedAt),
            version: row.version,
            agents: roster,
            ships: (shipsByFleet.get(row.id) ?? []).map(({ id, name }) => ({ id, name })),
          },
        ];
      }),
    );
  });
}

function findFleet(db: Db, organisationId: string, reference: string, lock = false) {
  const id = isUuid(reference) ? reference : undefined;
  const scope = and(
    eq(fleets.organisationId, organisationId),
    id ? or(eq(fleets.id, id), eq(fleets.slug, reference)) : eq(fleets.slug, reference),
  );
  const query = db
    .select()
    .from(fleets)
    .where(scope)
    .orderBy(id ? sql`(${fleets.id} = ${id}) desc` : fleets.id)
    .limit(1);
  return lock ? query.for("update") : query;
}

export const listFleets = (member: Member, query: { limit?: number; cursor?: string }) =>
  Effect.flatMap(Database, (db) =>
    db.transaction(
      (tx) =>
        Effect.gen(function* () {
          const { limit = 50, cursor } = query;
          const anchor = yield* readCursor(cursor);
          const rows = yield* tx
            .select({
              fleet: fleets,
              cursorTime: cursorTime(fleets.createdAt),
            })
            .from(fleets)
            .where(
              and(
                eq(fleets.organisationId, member.organisationId),
                anchor
                  ? sql`(${fleets.createdAt}, ${fleets.id}) < (${anchor.time}::timestamptz, ${anchor.id}::uuid)`
                  : undefined,
              ),
            )
            .orderBy(desc(fleets.createdAt), desc(fleets.id))
            .limit(limit + 1);
          const selected = rows.slice(0, limit);
          const map = yield* loadFleetDtos(
            tx,
            member.organisationId,
            selected.map(({ fleet }) => fleet),
          );
          const last = selected.at(-1);
          return {
            items: [...map.values()],
            nextCursor:
              last && rows.length > limit ? encodeCursor(last.cursorTime, last.fleet.id) : null,
          };
        }),
      { isolationLevel: "repeatable read", accessMode: "read only" },
    ),
  );

function readFleetDto(db: Db, row: FleetRow) {
  return Effect.map(loadFleetDtos(db, row.organisationId, [row]), (result) => result.get(row.id)!);
}

export const getFleet = (member: Member, reference: string) =>
  Effect.flatMap(Database, (db) =>
    db.transaction(
      (tx) =>
        Effect.gen(function* () {
          const [row] = yield* findFleet(tx, member.organisationId, reference);
          if (!row) return yield* Effect.fail(problem(404, "Fleet not found."));
          return yield* readFleetDto(tx, row);
        }),
      { isolationLevel: "repeatable read", accessMode: "read only" },
    ),
  );

const duplicateFleet = "A fleet already has that slug.";
const duplicateShip = "A ship in this fleet already has that name.";
const stale = (resource: string, version: number) =>
  problem(
    409,
    `The ${resource} changed. Reload it and retry. Current version is ${version}.`,
    version,
  );

export const createFleet = (member: Member, payload: typeof CreateFleet.Type) =>
  Effect.gen(function* () {
    if (member.role !== "admin" || member.agentRank)
      return yield* Effect.fail(
        problem(403, "Fleet creation requires a human Subspace administrator."),
      );
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        const [row] = yield* tx
          .insert(fleets)
          .values({ ...payload, organisationId: member.organisationId })
          .onConflictDoNothing({ target: [fleets.organisationId, fleets.slug] })
          .returning();
        if (!row) return yield* Effect.fail(problem(409, duplicateFleet));
        return yield* readFleetDto(tx, row);
      }),
    );
  });

export function hasConstraint(error: unknown, constraint: string): boolean {
  if (typeof error !== "object" || error === null) return false;
  if ("constraint" in error && error.constraint === constraint) return true;
  return "cause" in error && hasConstraint(error.cause, constraint);
}

export const updateFleet = (member: Member, reference: string, payload: typeof UpdateFleet.Type) =>
  Effect.gen(function* () {
    if (member.role !== "admin" || member.agentRank)
      return yield* Effect.fail(problem(403, "Fleet changes require organisation admin access."));
    const { version, ...fields } = payload;
    if (!Object.keys(fields).length)
      return yield* Effect.fail(validationProblem("fleet", "Supply at least one fleet field."));
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        const [old] = yield* findFleet(tx, member.organisationId, reference, true);
        if (!old) return yield* Effect.fail(problem(404, "Fleet not found."));
        if (version !== undefined && version !== old.version)
          return yield* Effect.fail(stale("fleet", old.version));
        if (Object.entries(fields).every(([key, value]) => old[key as keyof FleetRow] === value))
          return yield* readFleetDto(tx, old);
        const [row] = yield* tx
          .update(fleets)
          .set({ ...fields, updatedAt: sql`now()`, version: old.version + 1 })
          .where(eq(fleets.id, old.id))
          .returning()
          .pipe(
            Effect.catchIf(
              (error) => hasConstraint(error, "fleets_organisation_slug"),
              () => Effect.fail(problem(409, duplicateFleet)),
            ),
          );
        return yield* readFleetDto(tx, row!);
      }),
    );
  });

const shipScope = (member: Member, id: string) =>
  and(eq(ships.organisationId, member.organisationId), eq(ships.id, id));

export const listShips = (member: Member, id: string) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const [fleet] = yield* db
      .select({ id: fleets.id })
      .from(fleets)
      .where(and(eq(fleets.organisationId, member.organisationId), eq(fleets.id, id)));
    if (!fleet) return yield* Effect.fail(problem(404, "Fleet not found."));
    const rows = yield* db.select().from(ships).where(eq(ships.fleetId, id));
    return rows.sort(byShip).map(shipDto);
  });

export const createShip = (member: Member, id: string, name: string) =>
  Effect.gen(function* () {
    if (member.agentRank)
      return yield* Effect.fail(problem(403, "Only humans may manage fleets and ships."));
    const db = yield* Database;
    const [fleet] = yield* db
      .select({ id: fleets.id })
      .from(fleets)
      .where(and(eq(fleets.organisationId, member.organisationId), eq(fleets.id, id)));
    if (!fleet) return yield* Effect.fail(problem(404, "Fleet not found."));
    const [ship] = yield* db
      .insert(ships)
      .values({ organisationId: member.organisationId, fleetId: id, name })
      .onConflictDoNothing()
      .returning();
    if (!ship) return yield* Effect.fail(problem(409, duplicateShip));
    return shipDto(ship);
  });

export const renameShip = (member: Member, id: string, payload: typeof RenameShip.Type) =>
  Effect.gen(function* () {
    if (member.agentRank)
      return yield* Effect.fail(problem(403, "Only humans may manage fleets and ships."));
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        const [old] = yield* tx.select().from(ships).where(shipScope(member, id)).for("update");
        if (!old) return yield* Effect.fail(problem(404, "Ship not found."));
        if (payload.version !== undefined && payload.version !== old.version)
          return yield* Effect.fail(stale("ship", old.version));
        if (old.name === payload.name) return shipDto(old);
        const [ship] = yield* tx
          .update(ships)
          .set({ name: payload.name, version: old.version + 1, updatedAt: sql`now()` })
          .where(shipScope(member, id))
          .returning()
          .pipe(
            Effect.catchIf(
              (error) => hasConstraint(error, "ships_fleet_name"),
              () => Effect.fail(problem(409, duplicateShip)),
            ),
          );
        return shipDto(ship!);
      }),
    );
  });
