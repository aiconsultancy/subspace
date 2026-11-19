import { and, eq, sql } from "drizzle-orm";
import { Effect } from "effect";
import { HttpApiEndpoint, HttpApiGroup } from "effect/http-api";
import { withCrew } from "./crew";
import { Database } from "./db";
import { CurrentMember, type Member } from "./identity";
import {
  Comment,
  CommentLocation,
  EditComment,
  RemoveComment,
  ResolveComment,
} from "./issue-contract";
import { localMedia } from "./issue-media";
import { addActivity, commentDto, commentKey, lockIssues, readIssue } from "./issues";
import { validatedMentions } from "./mentions";
import { BearerAuth, handleDb, ValidationErrors, versionedApi } from "./middleware";
import { byKey, cursorAfter } from "./paging";
import { endpointErrors, problem } from "./problems";
import { readProject } from "./projects";
import { comments, issues, users } from "./schema";
import { Ref, Uuid } from "./validation";

type Db = (typeof Database)["Service"];

function commentRows(db: Db, member: Member, issueId?: string, commentId?: string) {
  const org = member.organisationId;
  return db
    .select({ comment: comments, author: users })
    .from(comments)
    .innerJoin(users, and(eq(users.organisationId, org), eq(users.id, comments.authorId)))
    .where(
      and(
        eq(comments.organisationId, org),
        issueId ? eq(comments.issueId, issueId) : undefined,
        commentId ? eq(comments.id, commentId) : undefined,
      ),
    );
}

function replyCount(db: Db, member: Member, issueId: string, rootId: string) {
  return Effect.map(
    db
      .select({ count: sql<number>`count(*)::int` })
      .from(comments)
      .where(
        and(
          eq(comments.organisationId, member.organisationId),
          eq(comments.issueId, issueId),
          eq(comments.parentCommentId, rootId),
        ),
      ),
    (rows) => rows[0]?.count ?? 0,
  );
}

export function getComment(member: Member, reference: string, id: string) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const issue = yield* readIssue(db, member, reference);
    const [row] = yield* commentRows(db, member, issue.id, id);
    if (!row) return yield* Effect.fail(problem(404, "Comment not found."));
    const count = row.comment.parentCommentId ? 0 : yield* replyCount(db, member, issue.id, id);
    return yield* commentDto(db, member, row.comment, row.author, count);
  });
}

export function locateComment(member: Member, id: string) {
  return Effect.flatMap(Database, (db) =>
    db.transaction(
      (tx) =>
        Effect.gen(function* () {
          const [target] = yield* commentRows(tx, member, undefined, id);
          if (!target) return yield* Effect.fail(problem(404, "Comment not found."));
          const issueId = target.comment.issueId;
          const rootId = target.comment.parentCommentId ?? id;
          const rows = yield* commentRows(tx, member, issueId);
          const root = rows.find(({ comment }) => comment.id === rootId)!;
          const replies = rows
            .filter(({ comment }) => comment.parentCommentId === rootId)
            .sort(byKey(commentKey));
          const focus = rootId === id ? 0 : replies.findIndex(({ comment }) => comment.id === id);
          const start = Math.max(focus - 10, 0);
          const end = rootId === id ? 20 : focus + 10;
          const window = replies.slice(start, end);
          return {
            issueId,
            requestedCommentId: id,
            root: yield* commentDto(tx, member, root.comment, root.author, replies.length),
            replies: yield* Effect.forEach(window, ({ comment, author }) =>
              commentDto(tx, member, comment, author, 0),
            ),
            beforeCursor: start ? `before:${window[0]!.comment.id}` : null,
            afterCursor:
              end < replies.length
                ? cursorAfter(
                    { org: member.organisationId, issue: issueId, root: rootId },
                    commentKey(window.at(-1)!),
                  )
                : null,
          };
        }),
      { isolationLevel: "repeatable read", accessMode: "read only" },
    ),
  );
}

type CommentChange =
  | ({ action: "edit" } & typeof EditComment.Type)
  | ({ action: "remove" } & typeof RemoveComment.Type)
  | ({ action: "resolve" } & typeof ResolveComment.Type);

export function changeComment(
  member: Member,
  reference: string,
  id: string,
  change: CommentChange,
) {
  return Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockIssues(tx, member);
        const actor = yield* withCrew(member, change.relayedCrew);
        const issue = yield* readIssue(tx, member, reference);
        const { project } = yield* readProject(tx, member, issue.projectId);
        const [row] = yield* commentRows(tx, member, issue.id, id);
        if (!row) return yield* Effect.fail(problem(404, "Comment not found."));
        const { comment, author } = row;
        if (issue.archivedAt || project.archivedAt)
          return yield* Effect.fail(
            problem(409, "Unarchive the issue and project before changing comments."),
          );
        if (change.action === "resolve" && comment.parentCommentId)
          return yield* Effect.fail(problem(400, "Resolution applies to root threads."));
        if (
          author.id !== member.id &&
          (change.action === "edit" || (change.action === "remove" && member.agentRank))
        )
          return yield* Effect.fail(problem(403, "You cannot change this comment."));
        if (change.version !== undefined && change.version !== comment.version)
          return yield* Effect.fail(problem(409, "The comment version is stale.", comment.version));
        const count = comment.parentCommentId ? 0 : yield* replyCount(tx, member, issue.id, id);
        if (comment.deletedAt) {
          if (change.action === "remove")
            return yield* commentDto(tx, member, comment, author, count);
          return yield* Effect.fail(problem(409, "The comment has been deleted."));
        }
        const now = new Date();
        let fields: Partial<typeof comments.$inferInsert>;
        let mentionUserIds: string[] = [];
        if (change.action === "edit") {
          const body = change.body.trim();
          if (body === comment.body) return yield* commentDto(tx, member, comment, author, count);
          mentionUserIds = yield* validatedMentions(tx, member, body, comment.body);
          yield* localMedia(tx, member, issue.id, body, true);
          fields = { body, editedAt: now };
        } else if (change.action === "resolve") {
          if (change.resolved === Boolean(comment.resolvedAt))
            return yield* commentDto(tx, member, comment, author, count);
          fields = {
            resolvedAt: change.resolved ? now : null,
            resolvedById: change.resolved ? member.id : null,
          };
        } else {
          fields = { body: "", deletedAt: now };
        }
        const [saved] = yield* tx
          .update(comments)
          .set({ ...fields, version: comment.version + 1 })
          .where(
            and(
              eq(comments.organisationId, member.organisationId),
              eq(comments.issueId, issue.id),
              eq(comments.id, id),
            ),
          )
          .returning();
        yield* tx
          .update(issues)
          .set({ lastActivityAt: sql`greatest(${issues.lastActivityAt}, now())` })
          .where(and(eq(issues.organisationId, member.organisationId), eq(issues.id, issue.id)));
        const kind = {
          edit: "comment.edited",
          remove: "comment.deleted",
          resolve: "comment.resolved",
        }[change.action];
        yield* addActivity(tx, actor, issue.id, kind, { commentId: id, mentionUserIds });
        return yield* commentDto(tx, member, saved!, author, count);
      }),
    ),
  );
}

const params = { reference: Ref, commentId: Uuid };
export const commentGroup = HttpApiGroup.make("comments")
  .add(
    HttpApiEndpoint.get("comment", "/issues/:reference/comments/:commentId", {
      params,
      success: Comment,
      error: endpointErrors,
    }),
    HttpApiEndpoint.put("editComment", "/issues/:reference/comments/:commentId", {
      params,
      payload: EditComment,
      success: Comment,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("removeComment", "/issues/:reference/comments/:commentId/delete", {
      params,
      payload: RemoveComment,
      success: Comment,
      error: endpointErrors,
    }),
    HttpApiEndpoint.put("resolveComment", "/issues/:reference/comments/:commentId/resolution", {
      params,
      payload: ResolveComment,
      success: Comment,
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("locateComment", "/issue-comments/:commentId/location", {
      params: { commentId: Uuid },
      success: CommentLocation,
      error: endpointErrors,
    }),
  )
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const commentHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof commentGroup>,
) =>
  handlers
    .handle("comment", ({ params }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          getComment(member, params.reference, params.commentId),
        ),
      ),
    )
    .handle("editComment", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          changeComment(member, params.reference, params.commentId, { ...payload, action: "edit" }),
        ),
      ),
    )
    .handle("removeComment", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          changeComment(member, params.reference, params.commentId, {
            ...payload,
            action: "remove",
          }),
        ),
      ),
    )
    .handle("resolveComment", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          changeComment(member, params.reference, params.commentId, {
            ...payload,
            action: "resolve",
          }),
        ),
      ),
    )
    .handle("locateComment", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => locateComment(member, params.commentId))),
    );
