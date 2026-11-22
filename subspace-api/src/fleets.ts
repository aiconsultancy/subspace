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
/** An IRC channel name: `#`, then no spaces, commas or control characters. */
export const IrcChannel = Schema.String.check(
  Schema.isMaxLength(64),
  Schema.isPattern(/^#[^\s,\p{Cc}]+$/u),
);
/** Hosts a plain-`http` Fleet API address may name: only this machine. */
const loopbackHosts = new Set(["localhost", "127.0.0.1", "[::1]"]);
const fleetApiUrlShape = /^[Hh][Tt][Tt][Pp][Ss]?:\/\/[^\s/?#@\\]+(?:\/[^\s?#\\]*)?$/u;
const fleetApiUrlLimit = 2048;
/**
 * Returns a Fleet API address in its stored form, or undefined when it is not
 * one. An address is an absolute `https` URL on any host, or `http` only on
 * loopback, with no credentials, query or fragment. The stored form has a
 * lower-case scheme and host and no trailing slash.
 */
export function normaliseFleetApiUrl(value: string): string | undefined {
  if (!fleetApiUrlShape.test(value)) return undefined;
  if (Array.from(value).some((c) => c.charCodeAt(0) < 33 || c.charCodeAt(0) === 127))
    return undefined;
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    return undefined;
  }
  if (url.username || url.password || url.search || url.hash || !url.hostname) return undefined;
  const plainLoopback = url.protocol === "http:" && loopbackHosts.has(url.hostname);
  if (url.protocol !== "https:" && !plainLoopback) return undefined;
  const normalised = `${url.origin}${url.pathname}`.replace(/\/+$/u, "");
  return normalised.length <= fleetApiUrlLimit ? normalised : undefined;
}
/**
 * The address of a fleet's Stargate Fleet API, which the desktop reads for the
 * fleet's live state. The desktop sends the fleet's token there, so plain
 * `http` is accepted only on loopback, such as the SSH tunnel's local end.
 */
export const FleetApiUrl = Schema.String.check(
  Schema.isMaxLength(fleetApiUrlLimit),
  Schema.isPattern(fleetApiUrlShape),
  Schema.makeFilter((value) => normaliseFleetApiUrl(value) !== undefined, {
    expected:
      "an absolute https URL, or http on localhost, 127.0.0.1 or [::1], without credentials, query or fragment",
  }),
);
const normaliseFields = <T extends { fleetApiUrl?: string | null }>(fields: T): T =>
  typeof fields.fleetApiUrl === "string"
    ? { ...fields, fleetApiUrl: normaliseFleetApiUrl(fields.fleetApiUrl) ?? fields.fleetApiUrl }
    : fields;
const editable = {
  slug: Slug.check(Schema.makeFilter((slug) => !isUuid(slug), { expected: "a slug, not a UUID" })),
  name: Name,
  ircHost: Host,
  ircPort: Port,
  ircTls: Schema.Boolean,
  ircWebsocketUrl: Schema.optional(Schema.NullOr(Websocket)),
  vmHost: Schema.optional(Schema.NullOr(Host)),
  starbaseChannel: Schema.optional(Schema.NullOr(IrcChannel)),
  commandChannel: Schema.optional(Schema.NullOr(IrcChannel)),
  fleetApiUrl: Schema.optional(Schema.NullOr(FleetApiUrl)),
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
  starbaseChannel: editable.starbaseChannel,
  commandChannel: editable.commandChannel,
  fleetApiUrl: editable.fleetApiUrl,
  version: Schema.optional(Version),
});
export const ShipName = Line(64);
const ShipChannel = Schema.optional(Schema.NullOr(IrcChannel));
export const CreateShip = Schema.Struct({ name: ShipName, channel: ShipChannel });
export const RenameShip = Schema.Struct({
  name: ShipName,
  channel: ShipChannel,
  version: Schema.optional(Version),
});
export const Ship = Schema.Struct({
  id: Uuid,
  fleetId: Uuid,
  name: Schema.String,
  channel: Schema.NullOr(Schema.String),
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
  ircNick: Schema.NullOr(Schema.String),
  shipChannel: Schema.NullOr(Schema.String),
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
  starbaseChannel: Schema.NullOr(Schema.String),
  commandChannel: Schema.NullOr(Schema.String),
  fleetApiUrl: Schema.NullOr(Schema.String),
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
  ships: Schema.Array(
    Schema.Struct({ id: Uuid, name: Schema.String, channel: Schema.NullOr(Schema.String) }),
  ),
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

export const shipDto = ({
  id,
  fleetId,
  name,
  channel,
  createdAt,
  updatedAt,
  version,
}: ShipRow) => ({
  id,
  fleetId,
  name,
  channel,
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
    const shipsById = new Map(shipRows.map((ship) => [ship.id, ship]));
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
                    (agent.agentShipId ? shipsById.get(agent.agentShipId)?.name : undefined) ??
                    agent.agentShip,
                  ircNick: agent.agentIrcNick,
                  shipChannel:
                    (agent.agentShipId ? shipsById.get(agent.agentShipId)?.channel : null) ?? null,
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
            starbaseChannel: row.starbaseChannel,
            commandChannel: row.commandChannel,
            fleetApiUrl: row.fleetApiUrl,
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
            ships: (shipsByFleet.get(row.id) ?? []).map(({ id, name, channel }) => ({
              id,
              name,
              channel,
            })),
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
          .values({ ...normaliseFields(payload), organisationId: member.organisationId })
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
    const { version, ...given } = payload;
    const fields = normaliseFields(given);
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

export const createShip = (member: Member, id: string, name: string, channel?: string | null) =>
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
      .values({ organisationId: member.organisationId, fleetId: id, name, channel })
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
        const channel = payload.channel === undefined ? old.channel : payload.channel;
        if (old.name === payload.name && old.channel === channel) return shipDto(old);
        const [ship] = yield* tx
          .update(ships)
          .set({ name: payload.name, channel, version: old.version + 1, updatedAt: sql`now()` })
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
