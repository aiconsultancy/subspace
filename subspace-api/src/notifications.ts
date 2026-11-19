import { and, eq, isNull, sql } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { HttpApiEndpoint, HttpApiGroup } from "effect/http-api";
import { recordAudit } from "./audit";
import { Database } from "./db";
import { CurrentMember, type Member } from "./identity";
import { readIssue } from "./issues";
import { BearerAuth, handleDb, ValidationErrors, versionedApi } from "./middleware";
import { byKey, page } from "./paging";
import { endpointErrors, problem } from "./problems";
import { Person, person } from "./projects";
import { issueNotifications, issueSubscriptions, users } from "./schema";
import { dateTime, PageQuery, pageOf, Ref, Timestamp, Uuid } from "./validation";

const Notification = Schema.Struct({
  id: Uuid,
  type: Schema.Literals(["mentioned", "issueCommentAdded", "issueStateChanged"]),
  title: Schema.String,
  body: Schema.String,
  isRead: Schema.Boolean,
  createdAt: Timestamp,
  actionUrl: Schema.String,
  relatedEntityType: Schema.Literals(["Issue", "IssueComment"]),
  relatedEntityId: Uuid,
  issueId: Uuid,
  commentId: Schema.NullOr(Uuid),
  actorUserId: Schema.NullOr(Uuid),
  changeId: Uuid,
});
const Subscription = Schema.Struct({
  issueId: Uuid,
  subscribed: Schema.Boolean,
  explicitChoice: Schema.Boolean,
});
export const NotificationQuery = Schema.Struct({
  ...PageQuery,
  unreadOnly: Schema.optional(Schema.Boolean),
});

type Row = typeof issueNotifications.$inferSelect;
function dto(row: Row) {
  return {
    id: row.id,
    type: row.type,
    title: row.title,
    body: row.body,
    isRead: Boolean(row.readAt),
    createdAt: dateTime(row.createdAt),
    actionUrl: row.commentId
      ? `/api/v1/issue-comments/${row.commentId}/location`
      : `/api/v1/issues/${row.issueId}`,
    relatedEntityType: row.commentId ? ("IssueComment" as const) : ("Issue" as const),
    relatedEntityId: row.commentId ?? row.issueId,
    issueId: row.issueId,
    commentId: row.commentId,
    actorUserId: row.actorUserId,
    changeId: row.changeId,
  };
}

const inbox = (member: Member, id?: string) =>
  and(
    eq(issueNotifications.organisationId, member.organisationId),
    eq(issueNotifications.userId, member.id),
    id ? eq(issueNotifications.id, id) : undefined,
  );

export function listNotifications(member: Member, query: typeof NotificationQuery.Type) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const rows = yield* db
      .select()
      .from(issueNotifications)
      .where(and(inbox(member), query.unreadOnly ? isNull(issueNotifications.readAt) : undefined));
    const key = (row: Row) =>
      `${String(Number.MAX_SAFE_INTEGER - row.createdAt.valueOf()).padStart(16, "0")}:${row.id}`;
    const result = yield* page(
      rows.sort(byKey(key)),
      { inbox: member.id, org: member.organisationId, unreadOnly: Boolean(query.unreadOnly) },
      key,
      query.limit,
      query.cursor,
    );
    return { ...result, items: result.items.map(dto) };
  });
}

export function markNotification(member: Member, id: string, read: boolean) {
  return Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        const [row] = yield* tx
          .select()
          .from(issueNotifications)
          .where(inbox(member, id))
          .for("update");
        if (!row) return yield* Effect.fail(problem(404, "Notification not found."));
        if (Boolean(row.readAt) === read) return dto(row);
        const readAt = read ? new Date() : null;
        yield* tx.update(issueNotifications).set({ readAt }).where(inbox(member, id));
        yield* recordAudit(tx, member, {
          action: read ? "notificationRead" : "notificationUnread",
          entityType: "IssueNotification",
          entityId: id,
          projectId: row.projectId,
          summary: "Changed notification read status.",
          changes: { issueId: row.issueId, isRead: read },
        });
        return dto({ ...row, readAt });
      }),
    ),
  );
}

export function markAllNotifications(member: Member) {
  return Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        const rows = yield* tx
          .update(issueNotifications)
          .set({ readAt: sql`now()` })
          .where(and(inbox(member), isNull(issueNotifications.readAt)))
          .returning({ projectId: issueNotifications.projectId });
        for (const [projectId, items] of Map.groupBy(rows, (row) => row.projectId)) {
          yield* recordAudit(tx, member, {
            action: "notificationsAllRead",
            entityType: "NotificationInbox",
            entityId: member.id,
            projectId,
            summary: "Marked inbox notifications read.",
            changes: { count: items.length },
          });
        }
        return { count: rows.length };
      }),
    ),
  );
}

export function chooseSubscription(member: Member, reference: string, subscribed: boolean) {
  return Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        const issue = yield* readIssue(tx, member, reference);
        const values = {
          organisationId: member.organisationId,
          issueId: issue.id,
          userId: member.id,
          subscribed,
          explicitChoice: true,
        };
        const [changed] = yield* tx
          .insert(issueSubscriptions)
          .values(values)
          .onConflictDoUpdate({
            target: [
              issueSubscriptions.organisationId,
              issueSubscriptions.issueId,
              issueSubscriptions.userId,
            ],
            set: { subscribed, explicitChoice: true },
            setWhere: sql`not ${issueSubscriptions.explicitChoice} or ${issueSubscriptions.subscribed} <> ${subscribed}`,
          })
          .returning();
        if (changed)
          yield* recordAudit(tx, member, {
            action: "issueSubscriptionChanged",
            entityType: "IssueSubscription",
            entityId: `${issue.id}:${member.id}`,
            projectId: issue.projectId,
            summary: "Changed an issue subscription.",
            changes: { subscribed, explicitChoice: true },
          });
        return { issueId: issue.id, subscribed, explicitChoice: true };
      }),
    ),
  );
}

export function listSubscribers(
  member: Member,
  reference: string,
  query: { limit?: number; cursor?: string },
) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const issue = yield* readIssue(db, member, reference);
    const rows = yield* db
      .select({ user: users })
      .from(issueSubscriptions)
      .innerJoin(
        users,
        and(
          eq(users.organisationId, member.organisationId),
          eq(users.id, issueSubscriptions.userId),
        ),
      )
      .where(
        and(
          eq(issueSubscriptions.organisationId, member.organisationId),
          eq(issueSubscriptions.issueId, issue.id),
          eq(issueSubscriptions.subscribed, true),
          eq(users.active, true),
        ),
      );
    const key = ({ user }: (typeof rows)[number]) => `${user.displayName.toLowerCase()}:${user.id}`;
    const result = yield* page(
      rows.sort(byKey(key)),
      { org: member.organisationId, subscribers: issue.id },
      key,
      query.limit,
      query.cursor,
    );
    return { ...result, items: result.items.map(({ user }) => person(user)!) };
  });
}

const notificationParams = { notificationId: Uuid };
const issueParams = { reference: Ref };

export const notificationsGroup = HttpApiGroup.make("notifications")
  .add(
    HttpApiEndpoint.get("notifications", "/notifications", {
      query: NotificationQuery,
      success: pageOf(Notification),
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("readAll", "/notifications/read-all", {
      success: Schema.Struct({ count: Schema.Int }),
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("read", "/notifications/:notificationId/read", {
      params: notificationParams,
      success: Notification,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("unread", "/notifications/:notificationId/unread", {
      params: notificationParams,
      success: Notification,
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("subscribers", "/issues/:reference/subscribers", {
      params: issueParams,
      query: PageQuery,
      success: pageOf(Person),
      error: endpointErrors,
    }),
    HttpApiEndpoint.put("subscribe", "/issues/:reference/subscription", {
      params: issueParams,
      payload: Schema.Struct({ subscribed: Schema.Boolean }),
      success: Subscription,
      error: endpointErrors,
    }),
  )
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const notificationHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof notificationsGroup>,
) =>
  handlers
    .handle("notifications", ({ query }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => listNotifications(member, query))),
    )
    .handle("readAll", () => handleDb(Effect.flatMap(CurrentMember, markAllNotifications)))
    .handle("read", ({ params }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          markNotification(member, params.notificationId, true),
        ),
      ),
    )
    .handle("unread", ({ params }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          markNotification(member, params.notificationId, false),
        ),
      ),
    )
    .handle("subscribers", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => listSubscribers(member, params.reference, query)),
      ),
    )
    .handle("subscribe", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          chooseSubscription(member, params.reference, payload.subscribed),
        ),
      ),
    );
