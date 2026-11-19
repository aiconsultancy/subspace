import { and, eq, inArray } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { type Database } from "./db";
import { type Member } from "./identity";
import { Version } from "./initiative-contract";
import { Person, person } from "./projects";
import { todos, users } from "./schema";
import { dateTime, dateTimeOrNull, Line, Timestamp, Uuid } from "./validation";

export const Todo = Schema.Struct({
  id: Uuid,
  issueId: Uuid,
  position: Schema.Int,
  text: Line(500),
  done: Schema.Boolean,
  doneAt: Schema.NullOr(Timestamp),
  createdBy: Person,
  updatedBy: Schema.NullOr(Person),
  doneBy: Schema.NullOr(Person),
  relayedCrew: Schema.Null,
  updatedRelayedCrew: Schema.Null,
  version: Version,
  createdAt: Timestamp,
  updatedAt: Timestamp,
}).annotate({ identifier: "IssueTodo" });

export const todoScope = (organisationId: string, issueId: string) =>
  and(eq(todos.organisationId, organisationId), eq(todos.issueId, issueId));

export const todoRows = (db: (typeof Database)["Service"], member: Member, issueId: string) =>
  db.select().from(todos).where(todoScope(member.organisationId, issueId)).orderBy(todos.position);

export function todoDtos(
  db: (typeof Database)["Service"],
  member: Member,
  rows: readonly (typeof todos.$inferSelect)[],
) {
  return Effect.gen(function* () {
    if (!rows.length) return [];
    const ids = [
      ...new Set(
        rows.flatMap((row) =>
          [row.createdById, row.updatedById, row.doneById].filter((id) => id != null),
        ),
      ),
    ];
    const profiles = yield* db
      .select()
      .from(users)
      .where(and(eq(users.organisationId, member.organisationId), inArray(users.id, ids)));
    const byId = new Map(profiles.map((row) => [row.id, row]));
    return rows.map((row) => ({
      id: row.id,
      issueId: row.issueId,
      position: row.position,
      text: row.text,
      done: row.doneAt !== null,
      doneAt: dateTimeOrNull(row.doneAt),
      createdBy: person(byId.get(row.createdById))!,
      updatedBy: row.updatedById ? person(byId.get(row.updatedById)) : null,
      doneBy: row.doneById ? person(byId.get(row.doneById)) : null,
      relayedCrew: null,
      updatedRelayedCrew: null,
      version: row.version,
      createdAt: dateTime(row.createdAt),
      updatedAt: dateTime(row.updatedAt),
    }));
  });
}
