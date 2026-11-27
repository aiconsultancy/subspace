import { and, eq, inArray, sql, type SQLWrapper } from "drizzle-orm";
import { Effect } from "effect";
import { marked } from "marked";
import { type Database } from "./db";
import { type Member } from "./identity";
import { problem } from "./problems";
import { comments, documentRevisions, issueDocuments, uploads } from "./schema";
import { isUuid } from "./validation";

type Db = (typeof Database)["Service"];
type Row = typeof uploads.$inferSelect;
const path = "/api/v1/issues/";
const routes = { image: "media/", file: "attachments/" };

type IssueText = { id: string; organisationId: string; description: string | null };

function references(body: SQLWrapper) {
  // Find both API and Markdown-editor references, including in link labels
  // or quoted text. A false positive is safer than deleting linked bytes.
  const route = sql`case when ${uploads.kind} = 'image' then ${routes.image} else ${routes.file} end`;
  return sql`(
    position(${path} || (${route}) || ${uploads.id}::text in lower(${body})) > 0
    or position('nett-media:' || ${uploads.id}::text in lower(${body})) > 0
  )`;
}

/** One bounded lookup for an attachment page or the file locked for deletion. */
function linkedMedia(db: Db, issue: IssueText, ids: readonly string[]) {
  return Effect.gen(function* () {
    if (!ids.length) return new Set<string>();
    const rows = yield* db
      .select({ id: uploads.id })
      .from(uploads)
      .where(
        and(
          eq(uploads.organisationId, issue.organisationId),
          eq(uploads.issueId, issue.id),
          inArray(uploads.id, ids),
          sql`(
          ${references(sql`${issue.description}::text`)}
          or exists (select 1 from ${comments}
            where ${comments.organisationId} = ${uploads.organisationId}
              and ${comments.issueId} = ${uploads.issueId}
              and ${comments.deletedAt} is null and ${references(comments.body)})
          or exists (select 1 from ${issueDocuments}
            join ${documentRevisions} on ${documentRevisions.documentId} = ${issueDocuments.id}
              and ${documentRevisions.organisationId} = ${issueDocuments.organisationId}
            where ${issueDocuments.organisationId} = ${uploads.organisationId}
              and ${issueDocuments.issueId} = ${uploads.issueId}
              and ${references(documentRevisions.content)})
        )`,
        ),
      );
    return new Set(rows.map(({ id }) => id));
  });
}

export function mediaRemoval(
  db: Db,
  issue: IssueText & { archivedAt: Date | null },
  ids: readonly string[],
) {
  return Effect.gen(function* () {
    if (issue.archivedAt) return (_id: string): "archived" => "archived";
    const linked = yield* linkedMedia(db, issue, ids);
    return (id: string): "linked" | "available" => (linked.has(id) ? "linked" : "available");
  });
}

export const mediaUrl = (row: Row) =>
  `${path}${routes[row.kind === "image" ? "image" : "file"]}${row.id}`;

export function mediaDescriptor(row: Row) {
  return {
    fileName: row.fileName,
    contentType: row.contentType,
    sizeBytes: row.sizeBytes,
    sha256: row.sha256,
    url: mediaUrl(row),
    mediaId: row.id,
    version: 1,
    kind: row.kind === "image" ? ("image" as const) : ("file" as const),
    expiresAt: null,
    publicFileId: row.id,
  };
}

export function localMedia(
  db: Db,
  member: Member,
  issueId: string,
  body?: string | null,
  lock = false,
) {
  return Effect.gen(function* () {
    if (!body) return [];
    // Match only the API's managed paths. Absolute external URLs and other
    // links remain ordinary Markdown and are never fetched by this service.
    const found = [
      ...body.matchAll(
        /(?:^|[\s(<])\/api\/v1\/issues\/(media|attachments)\/([0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12})(?=[)\s>]|$)/giu,
      ),
    ].map((match) => ({
      id: match[2]!.toLowerCase(),
      kind: match[1]?.toLowerCase() === "media" ? "image" : "file",
    }));
    marked.walkTokens(marked.lexer(body), (token) => {
      if (
        (token.type !== "image" && token.type !== "link") ||
        !token.href.startsWith("nett-media:")
      )
        return;
      const id = token.href.slice("nett-media:".length);
      if (isUuid(id))
        found.push({ id: id.toLowerCase(), kind: token.type === "image" ? "image" : "" });
    });
    if (!found.length) return [];
    const ids = [...new Set(found.map(({ id }) => id))];
    if (ids.length > 100)
      return yield* Effect.fail(problem(400, "Use at most 100 managed files per message."));
    const select = db
      .select()
      .from(uploads)
      .where(
        and(
          eq(uploads.organisationId, member.organisationId),
          eq(uploads.issueId, issueId),
          eq(uploads.state, "ready"),
          inArray(uploads.id, ids),
        ),
      )
      .orderBy(uploads.id);
    const rows = lock ? yield* select.for("share") : yield* select;
    const byId = new Map(rows.map((row) => [row.id, row]));
    const descriptors = found.flatMap((match) => {
      const row = byId.get(match.id);
      if (!row || (match.kind && row.kind !== match.kind)) return [];
      return [mediaDescriptor(row)];
    });
    if (lock && descriptors.length !== found.length) {
      return yield* Effect.fail(
        problem(400, "A referenced file does not belong to this issue or is unavailable."),
      );
    }
    return [...new Map(descriptors.map((media) => [media.mediaId, media])).values()];
  });
}
