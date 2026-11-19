import { and, asc, eq, isNull, lt, or, sql } from "drizzle-orm";
import { Effect } from "effect";
import { HttpServerResponse } from "effect/http";
import { Database } from "./db";
import { fileTypes, imageTypes, validContent } from "./file-validation";
import { type Member } from "./identity";
import { readIssue } from "./issues";
import { mediaDescriptor, mediaUrl } from "./issue-media";
import { type ChatUploadInput } from "./media-contract";
import { byKey, page } from "./paging";
import { problem } from "./problems";
import { person, readProject } from "./projects";
import { comments, documentRevisions, issueDocuments, uploads, users } from "./schema";
import { Storage, storageOperation } from "./storage";
import { dateTime, isUuid } from "./validation";

type Row = typeof uploads.$inferSelect;
type Db = (typeof Database)["Service"];
type Kind = "image" | "file";
const maxChatBytes = 25_000_000;
const maxIssueBytes = 10_000_000;
const chatExpiration = 30 * 86_400_000;

const scope = (member: Member) => eq(uploads.organisationId, member.organisationId);
export const digest = (data: Uint8Array) =>
  new Bun.CryptoHasher("sha256").update(data).digest("hex");

function reference(id: string) {
  return `UPL-${Buffer.from(id.replaceAll("-", ""), "hex").toString("base64url")}`;
}
function uuidFromReference(value: string) {
  if (!/^UPL-[A-Za-z0-9_-]{22}$/u.test(value)) return null;
  const hex = Buffer.from(value.slice(4), "base64url").toString("hex");
  const id = `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
  return isUuid(id) && reference(id) === value ? id : null;
}
const objectKey = (row: Row) => `${row.organisationId}-${row.id}`;
const url = (row: Row) =>
  row.kind === "chat" ? `/api/v1/chat/uploads/${reference(row.clientId!)}/content` : mediaUrl(row);

const fileFacts = (row: Row) => ({
  fileName: row.fileName,
  contentType: row.contentType,
  sizeBytes: row.sizeBytes,
  sha256: row.sha256,
  url: url(row),
});
const chatDto = (row: Row) => ({
  ...fileFacts(row),
  id: row.clientId!,
  reference: reference(row.clientId!),
  uploadedByUserId: row.uploadedById,
  expiresAt: dateTime(row.expiresAt!),
  createdAt: dateTime(row.createdAt),
});
const imageDto = (row: Row, member: Member) => ({
  ...fileFacts(row),
  id: row.id,
  issueId: row.issueId!,
  uploadedBy: person(member)!,
  createdAt: dateTime(row.createdAt),
  relayedCrew: null,
});
function readChat(db: Db, member: Member, key: string) {
  return Effect.gen(function* () {
    const id = uuidFromReference(key);
    if (!id) return yield* Effect.fail(problem(404, "Upload not found."));
    const [row] = yield* db
      .select()
      .from(uploads)
      .where(and(scope(member), eq(uploads.clientId, id), eq(uploads.kind, "chat")));
    if (row?.expiresAt && row.expiresAt.valueOf() <= Date.now() && row.state !== "pending") {
      return yield* Effect.fail(problem(410, "This upload has expired."));
    }
    if (row?.state !== "ready") return yield* Effect.fail(problem(404, "Upload not found."));
    return row;
  });
}

function commitBlob(db: Db, store: (typeof Storage)["Service"], row: Row, bytes: Uint8Array) {
  return Effect.gen(function* () {
    yield* storageOperation(() => store.put(objectKey(row), bytes));
    const [ready] = yield* db
      .update(uploads)
      .set({ state: "ready" })
      .where(
        and(
          eq(uploads.id, row.id),
          eq(uploads.organisationId, row.organisationId),
          eq(uploads.state, "pending"),
        ),
      )
      .returning();
    if (!ready)
      return yield* Effect.fail(
        problem(409, "The pending upload changed before it could be saved."),
      );
    return ready;
  });
}

export const createChatUpload = (member: Member, input: typeof ChatUploadInput.Type) =>
  Effect.gen(function* () {
    const bytes = Buffer.from(input.base64, "base64");
    if (
      bytes.length > maxChatBytes ||
      bytes.toString("base64") !== input.base64 ||
      !validContent(input.fileName, input.contentType, bytes)
    ) {
      return yield* Effect.fail(
        problem(
          400,
          "The upload must contain supported file bytes matching its name and media type.",
        ),
      );
    }
    const db = yield* Database;
    const store = yield* Storage;
    const sha256 = digest(bytes);
    const [newRow] = yield* db
      .insert(uploads)
      .values({
        organisationId: member.organisationId,
        uploadedById: member.id,
        kind: "chat",
        clientId: input.uploadId,
        fileName: input.fileName,
        contentType: input.contentType,
        sizeBytes: bytes.length,
        sha256,
        expiresAt: new Date(Date.now() + chatExpiration),
      })
      .onConflictDoNothing({ target: [uploads.organisationId, uploads.clientId] })
      .returning();
    if (!newRow) {
      const [current] = yield* db
        .select()
        .from(uploads)
        .where(and(scope(member), eq(uploads.clientId, input.uploadId)));
      if (
        current?.uploadedById !== member.id ||
        current.fileName !== input.fileName ||
        current.contentType !== input.contentType ||
        current.sha256 !== sha256
      ) {
        return yield* Effect.fail(problem(409, "An upload with that ID already exists."));
      }
      if (current.expiresAt && current.expiresAt.valueOf() <= Date.now()) {
        return yield* Effect.fail(problem(410, "This upload has expired."));
      }
      if (current.state !== "ready")
        return yield* Effect.fail(problem(409, "The upload is currently unavailable."));
      return chatDto(current);
    }
    return chatDto(yield* commitBlob(db, store, newRow, bytes));
  });

export const chatUpload = (member: Member, ref: string) =>
  Effect.flatMap(Database, (db) => Effect.map(readChat(db, member, ref), chatDto));

function readBytes(row: Row) {
  return Effect.gen(function* () {
    const store = yield* Storage;
    const bytes = yield* storageOperation(() => store.get(objectKey(row))).pipe(
      Effect.mapError(() => problem(503, "The file is temporarily unavailable.")),
    );
    if (bytes.byteLength !== row.sizeBytes || digest(bytes) !== row.sha256) {
      return yield* Effect.fail(problem(503, "The file could not be verified."));
    }
    return HttpServerResponse.uint8Array(bytes, {
      contentType: row.contentType,
      headers: {
        "X-Content-Type-Options": "nosniff",
        "Content-Disposition": `attachment; filename*=UTF-8''${encodeURIComponent(row.fileName)}`,
      },
    });
  });
}

export const chatBytes = (member: Member, ref: string) =>
  Effect.flatMap(Database, (db) => Effect.flatMap(readChat(db, member, ref), readBytes));

export const resolveUploads = (member: Member, refs: readonly string[]) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const items = yield* Effect.forEach(refs, (reference) =>
      readChat(db, member, reference).pipe(
        Effect.map((row) => ({
          reference,
          kind: "chatUpload" as const,
          status: "available" as const,
          upload: chatDto(row),
        })),
        Effect.catchIf(
          (error) => "status" in error,
          (error) =>
            Effect.succeed({
              reference,
              kind: "chatUpload" as const,
              status: error.status === 410 ? ("expired" as const) : ("missing" as const),
              upload: null,
            }),
        ),
      ),
    );
    return { items };
  });

export const deleteChat = (member: Member, ref: string) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const row = yield* readChat(db, member, ref);
    if (row.uploadedById !== member.id) {
      return yield* Effect.fail(problem(403, "Only the uploader may delete this upload."));
    }
    yield* db
      .update(uploads)
      .set({ state: "deleted" })
      .where(and(scope(member), eq(uploads.id, row.id)));
    return true;
  });

export const issueMedia = (
  member: Member,
  ref: string,
  kind: Kind,
  query: { limit?: number; cursor?: string },
) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const issue = yield* readIssue(db, member, ref);
    const rows = yield* db
      .select({ upload: uploads, uploader: users })
      .from(uploads)
      .innerJoin(
        users,
        and(eq(users.organisationId, member.organisationId), eq(users.id, uploads.uploadedById)),
      )
      .where(
        and(
          scope(member),
          eq(uploads.issueId, issue.id),
          eq(uploads.kind, kind),
          eq(uploads.state, "ready"),
        ),
      );
    const key = ({ upload }: (typeof rows)[number]) => `${dateTime(upload.createdAt)}:${upload.id}`;
    return yield* page(
      rows.sort(byKey(key)),
      { org: member.organisationId, media: issue.id, kind },
      key,
      query.limit,
      query.cursor,
    );
  });

export const listImages = (
  member: Member,
  ref: string,
  query: { limit?: number; cursor?: string },
) =>
  Effect.map(issueMedia(member, ref, "image", query), ({ items, nextCursor }) => ({
    items: items.map(({ upload, uploader }) => imageDto(upload, uploader)),
    nextCursor,
  }));

export const listFiles = (
  member: Member,
  ref: string,
  query: { limit?: number; cursor?: string },
) =>
  Effect.map(issueMedia(member, ref, "file", query), ({ items, nextCursor }) => ({
    items: items.map(({ upload }) => mediaDescriptor(upload)),
    nextCursor,
  }));

export const addIssueMedia = (
  member: Member,
  ref: string,
  kind: Kind,
  file: { path: string; name: string; contentType: string },
) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const store = yield* Storage;
    const local = Bun.file(file.path);
    const image = kind === "image";
    const allowed: readonly string[] = image ? imageTypes : fileTypes;
    if (
      local.size > (image ? 10 * 1024 * 1024 : maxIssueBytes) ||
      !allowed.includes(file.contentType)
    ) {
      return yield* Effect.fail(problem(400, "This file type or size is not supported."));
    }
    const bytes = Buffer.from(yield* storageOperation(() => local.arrayBuffer()));
    if (!validContent(file.name, file.contentType, bytes)) {
      return yield* Effect.fail(problem(400, "The file does not match its name and media type."));
    }
    const issue = yield* readIssue(db, member, ref);
    const { project } = yield* readProject(db, member, issue.projectId);
    if (issue.archivedAt || project.archivedAt)
      return yield* Effect.fail(
        problem(409, "Unarchive the issue and project before uploading files."),
      );
    const [row] = yield* db
      .insert(uploads)
      .values({
        organisationId: member.organisationId,
        kind,
        uploadedById: member.id,
        issueId: issue.id,
        fileName: file.name,
        contentType: file.contentType,
        sizeBytes: bytes.length,
        sha256: digest(bytes),
      })
      .returning();
    if (!row) return yield* Effect.fail(problem(500, "The file could not be registered."));
    return yield* commitBlob(db, store, row, bytes);
  });

export const addImage = (member: Member, ref: string, file: Parameters<typeof addIssueMedia>[3]) =>
  Effect.map(addIssueMedia(member, ref, "image", file), (row) => imageDto(row, member));

export const addFile = (member: Member, ref: string, file: Parameters<typeof addIssueMedia>[3]) =>
  Effect.map(addIssueMedia(member, ref, "file", file), mediaDescriptor);

export const issueBytes = (member: Member, id: string, kind: Kind) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const [row] = yield* db
      .select()
      .from(uploads)
      .where(
        and(scope(member), eq(uploads.id, id), eq(uploads.kind, kind), eq(uploads.state, "ready")),
      );
    if (!row) return yield* Effect.fail(problem(404, "Issue file not found."));
    return yield* readBytes(row);
  });

export const deleteMedia = (member: Member, id: string, kind: Kind) =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        const filter = and(scope(member), eq(uploads.id, id), eq(uploads.kind, kind));
        const [row] = yield* tx.select().from(uploads).where(filter).for("update");
        if (!row || row.state === "pending")
          return yield* Effect.fail(problem(404, "Issue file not found."));
        if (row.state === "deleted") return false;
        const issue = yield* readIssue(tx, member, row.issueId!);
        if (issue.archivedAt)
          return yield* Effect.fail(problem(409, "Unarchive the issue before deleting its files."));
        const schemes = [url(row), `nett-media:${row.id}`];
        const [inUse] = yield* tx
          .select({ id: comments.id })
          .from(comments)
          .where(
            and(
              eq(comments.organisationId, member.organisationId),
              eq(comments.issueId, issue.id),
              isNull(comments.deletedAt),
              or(...schemes.map((ref) => sql`position(${ref} in lower(${comments.body})) > 0`)),
            ),
          )
          .limit(1);
        const [usedInDocument] = yield* tx
          .select({ id: issueDocuments.id })
          .from(issueDocuments)
          .innerJoin(
            documentRevisions,
            and(
              eq(documentRevisions.organisationId, member.organisationId),
              eq(documentRevisions.documentId, issueDocuments.id),
            ),
          )
          .where(
            and(
              eq(issueDocuments.organisationId, member.organisationId),
              eq(issueDocuments.issueId, issue.id),
              or(
                ...schemes.map(
                  (ref) => sql`position(${ref} in lower(${documentRevisions.content})) > 0`,
                ),
              ),
            ),
          )
          .limit(1);
        if (
          inUse ||
          usedInDocument ||
          schemes.some((ref) => issue.description?.toLowerCase().includes(ref))
        )
          return yield* Effect.fail(
            problem(409, "The file is referenced by an issue, comment or document."),
          );
        yield* tx.update(uploads).set({ state: "deleted" }).where(filter);
        return true;
      }),
    );
  });

export const purgeUploads = Effect.gen(function* () {
  const db = yield* Database;
  const store = yield* Storage;
  const pendingCutoff = new Date(Date.now() - 3_600_000);
  const expired = yield* db
    .select()
    .from(uploads)
    .where(
      and(
        isNull(uploads.purgedAt),
        or(
          eq(uploads.state, "deleted"),
          lt(uploads.expiresAt, new Date()),
          and(eq(uploads.state, "pending"), lt(uploads.createdAt, pendingCutoff)),
        ),
      ),
    )
    .orderBy(asc(uploads.createdAt))
    .limit(100);
  yield* Effect.forEach(
    expired,
    (row) =>
      storageOperation(() => store.delete(objectKey(row))).pipe(
        Effect.flatMap(() =>
          db
            .update(uploads)
            .set({ state: "deleted", purgedAt: sql`now()` })
            .where(and(eq(uploads.organisationId, row.organisationId), eq(uploads.id, row.id))),
        ),
        Effect.catch(() => Effect.logWarning("An expired or deleted file could not be purged.")),
      ),
    { discard: true },
  );
});
