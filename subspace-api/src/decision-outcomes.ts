import { and, desc, eq, isNull, sql } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { Database } from "./db";
import type { Member } from "./identity";
import { cursorTime, encodeCursor, readCursor } from "./paging";
import { problem } from "./problems";
import { decisionOutcomes, decisions, users } from "./schema";
import { dateTime, Timestamp, Uuid } from "./validation";

export const DecisionOutcome = Schema.Struct({
  id: Uuid,
  identifier: Schema.String,
  recordedByUserId: Uuid,
  status: Schema.Literals(["accepted", "rejected"]),
  createdAt: Timestamp,
});

function canDispatch(member: Member, fleetId: string) {
  return member.active && member.agentRank === "rearAdmiral" && member.agentFleetId === fleetId;
}

function scope(member: Member, fleetId: string) {
  return and(
    eq(decisionOutcomes.organisationId, member.organisationId),
    eq(decisionOutcomes.fleetId, fleetId),
  );
}

export function listDecisionOutcomes(
  member: Member,
  fleetId: string,
  query: { limit?: number; cursor?: string },
) {
  return Effect.gen(function* () {
    if (!canDispatch(member, fleetId)) {
      return yield* Effect.fail(
        problem(403, "Use the active rear admiral identity for this fleet."),
      );
    }
    const db = yield* Database;
    const { limit = 50, cursor } = query;
    const anchor = yield* readCursor(cursor);
    const rows = yield* db
      .select({
        id: decisionOutcomes.decisionId,
        identifier: decisions.identifier,
        recordedByUserId: decisionOutcomes.recordedById,
        status: decisionOutcomes.status,
        createdAt: decisionOutcomes.createdAt,
        cursorTime: cursorTime(decisionOutcomes.createdAt),
      })
      .from(decisionOutcomes)
      .innerJoin(
        decisions,
        and(
          eq(decisions.organisationId, decisionOutcomes.organisationId),
          eq(decisions.id, decisionOutcomes.decisionId),
        ),
      )
      .innerJoin(
        users,
        and(
          eq(users.organisationId, decisionOutcomes.organisationId),
          eq(users.id, decisionOutcomes.recordedById),
          eq(users.agentFleetId, decisionOutcomes.fleetId),
        ),
      )
      .where(
        and(
          scope(member, fleetId),
          eq(decisionOutcomes.recordedByRank, "captain"),
          eq(users.active, true),
          eq(users.agentRank, "captain"),
          isNull(decisionOutcomes.deliveredAt),
          anchor
            ? sql`(${decisionOutcomes.createdAt}, ${decisionOutcomes.decisionId}) < (${anchor.time}::timestamptz, ${anchor.id}::uuid)`
            : undefined,
        ),
      )
      .orderBy(desc(decisionOutcomes.createdAt), desc(decisionOutcomes.decisionId))
      .limit(limit + 1);
    const selected = rows.slice(0, limit);
    const last = selected.at(-1);
    return {
      items: selected.map((row) => ({
        id: row.id,
        identifier: row.identifier,
        recordedByUserId: row.recordedByUserId,
        status: row.status,
        createdAt: dateTime(row.createdAt),
      })),
      nextCursor: last && rows.length > limit ? encodeCursor(last.cursorTime, last.id) : null,
    };
  });
}

export function acknowledgeDecisionOutcome(member: Member, fleetId: string, id: string) {
  return Effect.gen(function* () {
    if (!canDispatch(member, fleetId)) {
      return yield* Effect.fail(
        problem(403, "Use the active rear admiral identity for this fleet."),
      );
    }
    const db = yield* Database;
    const [matched] = yield* db
      .update(decisionOutcomes)
      .set({ deliveredAt: sql`coalesce(${decisionOutcomes.deliveredAt}, now())` })
      .where(
        and(
          scope(member, fleetId),
          eq(decisionOutcomes.decisionId, id),
          eq(decisionOutcomes.recordedByRank, "captain"),
        ),
      )
      .returning({ id: decisionOutcomes.decisionId });
    if (!matched)
      return yield* Effect.fail(problem(404, "Decision outcome was not found in this fleet."));
  });
}
