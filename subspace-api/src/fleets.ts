import { and, asc, eq, sql } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { Database } from "./db";
import { type Member } from "./identity";
import { problem } from "./problems";
import { fleets } from "./schema";
import { dateTime, Host, Name, Port, Slug, Timestamp, Uuid } from "./validation";

const Websocket = Schema.String.check(
  Schema.isMaxLength(2048),
  Schema.isPattern(/^wss?:\/\/[^\s/@?#]+(?:[/?#][^\s]*)?$/u),
);
const editable = {
  slug: Slug,
  name: Name,
  ircHost: Host,
  ircPort: Port,
  ircTls: Schema.Boolean,
  ircWebsocketUrl: Schema.optional(Schema.NullOr(Websocket)),
  rearAdmiralName: Name,
  captainNames: Schema.Array(Name).check(Schema.isMaxLength(100)),
  vmHost: Schema.optional(Schema.NullOr(Host)),
};

export const CreateFleet = Schema.Struct(editable);
export const UpdateFleet = CreateFleet.mapFields((fields) => ({
  slug: Schema.optional(fields.slug),
  name: Schema.optional(fields.name),
  ircHost: Schema.optional(fields.ircHost),
  ircPort: Schema.optional(fields.ircPort),
  ircTls: Schema.optional(fields.ircTls),
  ircWebsocketUrl: fields.ircWebsocketUrl,
  rearAdmiralName: Schema.optional(fields.rearAdmiralName),
  captainNames: Schema.optional(fields.captainNames),
  vmHost: fields.vmHost,
}));
export const Fleet = Schema.Struct({
  ...editable,
  ircWebsocketUrl: Schema.NullOr(Websocket),
  vmHost: Schema.NullOr(Host),
  id: Uuid,
  irc: Schema.Struct({
    host: Host,
    port: Port,
    tls: Schema.Boolean,
    websocketUrl: Schema.NullOr(Websocket),
  }),
  createdAt: Timestamp,
  updatedAt: Timestamp,
}).annotate({ identifier: "Fleet" });
export const FleetRef = Schema.String.check(Schema.isMaxLength(80));
export const FleetAssignment = Schema.Struct({ fleetId: Schema.NullOr(Uuid) });

export const fleetDto = (row: typeof fleets.$inferSelect) => ({
  id: row.id,
  slug: row.slug,
  name: row.name,
  ircHost: row.ircHost,
  ircPort: row.ircPort,
  ircTls: row.ircTls,
  ircWebsocketUrl: row.ircWebsocketUrl,
  rearAdmiralName: row.rearAdmiralName,
  captainNames: row.captainNames,
  vmHost: row.vmHost,
  irc: { host: row.ircHost, port: row.ircPort, tls: row.ircTls, websocketUrl: row.ircWebsocketUrl },
  createdAt: dateTime(row.createdAt),
  updatedAt: dateTime(row.updatedAt),
});

function byReference(organisationId: string, reference: string) {
  return and(
    eq(fleets.organisationId, organisationId),
    /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(reference)
      ? eq(fleets.id, reference)
      : eq(fleets.slug, reference),
  );
}

export const listFleets = (member: Member) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const rows = yield* db
      .select()
      .from(fleets)
      .where(eq(fleets.organisationId, member.organisationId))
      .orderBy(asc(fleets.name), asc(fleets.id));
    return { items: rows.map(fleetDto) };
  });

export const getFleet = (member: Member, reference: string) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const [row] = yield* db
      .select()
      .from(fleets)
      .where(byReference(member.organisationId, reference));
    if (!row) return yield* Effect.fail(problem(404, "Fleet not found."));
    return fleetDto(row);
  });

export const createFleet = (member: Member, payload: typeof CreateFleet.Type) =>
  Effect.gen(function* () {
    if (member.role !== "admin")
      return yield* Effect.fail(
        problem(403, "Fleet registration requires organisation admin access."),
      );
    const db = yield* Database;
    const [row] = yield* db
      .insert(fleets)
      .values({ ...payload, organisationId: member.organisationId })
      .onConflictDoNothing({ target: [fleets.organisationId, fleets.slug] })
      .returning();
    if (!row) return yield* Effect.fail(problem(409, "A fleet already has that slug."));
    return fleetDto(row);
  });

function hasConstraint(error: unknown, constraint: string): boolean {
  if (typeof error !== "object" || error === null) return false;
  if ("constraint" in error && error.constraint === constraint) return true;
  return "cause" in error && hasConstraint(error.cause, constraint);
}

export const updateFleet = (member: Member, reference: string, payload: typeof UpdateFleet.Type) =>
  Effect.gen(function* () {
    if (member.role !== "admin")
      return yield* Effect.fail(problem(403, "Fleet changes require organisation admin access."));
    if (!Object.keys(payload).length)
      return yield* Effect.fail(problem(400, "Supply at least one fleet field."));
    const db = yield* Database;
    const [row] = yield* db
      .update(fleets)
      .set({ ...payload, updatedAt: sql`now()` })
      .where(byReference(member.organisationId, reference))
      .returning()
      .pipe(
        Effect.catchIf(
          (error) => hasConstraint(error, "fleets_organisation_slug"),
          () => Effect.fail(problem(409, "A fleet already has that slug.")),
        ),
      );
    if (!row) return yield* Effect.fail(problem(404, "Fleet not found."));
    return fleetDto(row);
  });
