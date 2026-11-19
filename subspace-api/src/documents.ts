import { and, eq, isNull, sql } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { HttpApiEndpoint, HttpApiGroup, HttpApiSchema } from "effect/http-api";
import { Database } from "./db";
import { CurrentMember, type Member } from "./identity";
import { Version } from "./initiative-contract";
import { localMedia } from "./issue-media";
import { addActivity, lockIssues, readIssue } from "./issues";
import { BearerAuth, handleDb, ValidationErrors, versionedApi } from "./middleware";
import { byKey, page } from "./paging";
import { endpointErrors, problem } from "./problems";
import { person, Person, readProject } from "./projects";
import {
  documentCounters,
  documentRevisions,
  issueDocuments,
  issues,
  reviewKinds,
  users,
} from "./schema";
import {
  dateTime,
  dateTimeOrNull,
  isUuid,
  Line,
  NullableString,
  PageQuery,
  pageOf,
  Paragraph,
  Ref,
  Timestamp,
  Uuid,
} from "./validation";

export const ReviewKind = Schema.Literals(reviewKinds);
const RevisionBody = {
  title: Line(200),
  content: Paragraph(65_536),
  note: Schema.optional(Schema.NullOr(Paragraph(2000))),
};
export const CreateDocument = Schema.Struct({
  type: ReviewKind,
  subtype: Schema.optional(Schema.NullOr(Line(120))),
  ...RevisionBody,
});
export const EditDocument = Schema.Struct({
  baseRevision: Version,
  title: Schema.optional(RevisionBody.title),
  content: Schema.optional(RevisionBody.content),
  note: RevisionBody.note,
});
const Metadata = Schema.Struct({
  id: Uuid,
  issueId: Uuid,
  reference: Ref,
  type: ReviewKind,
  subtype: NullableString,
  title: Line(200),
  revision: Version,
  authorUserId: Uuid,
  author: Person,
  createdAt: Timestamp,
  updatedAt: Timestamp,
  archived: Schema.Boolean,
  archivedAt: Schema.NullOr(Timestamp),
});
const Document = Schema.Struct({ ...Metadata.fields, content: Schema.String });
const RevisionMetadata = Schema.Struct({
  number: Version,
  title: Line(200),
  authorUserId: Uuid,
  author: Person,
  createdAt: Timestamp,
  note: NullableString,
});
const Revision = Schema.Struct({ ...RevisionMetadata.fields, content: Schema.String });
export const DocumentQuery = Schema.Struct({
  ...PageQuery,
  type: Schema.optional(ReviewKind),
  includeArchived: Schema.optional(Schema.Boolean),
});

type Db = (typeof Database)["Service"];
type Doc = typeof issueDocuments.$inferSelect;
type Rev = typeof documentRevisions.$inferSelect;
const docScope = (member: Member, id?: string) =>
  and(
    eq(issueDocuments.organisationId, member.organisationId),
    id ? eq(issueDocuments.id, id) : undefined,
  );

function documents(db: Db, member: Member) {
  return db
    .select({ doc: issueDocuments, rev: documentRevisions, author: users })
    .from(issueDocuments)
    .innerJoin(
      documentRevisions,
      and(
        eq(documentRevisions.organisationId, member.organisationId),
        eq(documentRevisions.documentId, issueDocuments.id),
        eq(documentRevisions.number, issueDocuments.currentRevision),
      ),
    )
    .innerJoin(
      users,
      and(eq(users.organisationId, member.organisationId), eq(users.id, issueDocuments.authorId)),
    );
}

function docDto({ doc, rev, author }: { doc: Doc; rev: Rev; author: Member }) {
  return {
    id: doc.id,
    issueId: doc.issueId,
    reference: doc.reference,
    type: doc.type,
    subtype: doc.subtype,
    title: rev.title,
    content: rev.content,
    revision: doc.currentRevision,
    authorUserId: author.id,
    author: person(author)!,
    createdAt: dateTime(doc.createdAt),
    updatedAt: dateTime(doc.updatedAt),
    archived: Boolean(doc.archivedAt),
    archivedAt: dateTimeOrNull(doc.archivedAt),
  };
}

export function readDocument(db: Db, member: Member, reference: string) {
  return Effect.gen(function* () {
    const [row] = yield* documents(db, member).where(
      and(
        docScope(member),
        isUuid(reference)
          ? eq(issueDocuments.id, reference)
          : eq(issueDocuments.reference, reference.toUpperCase()),
      ),
    );
    if (!row) return yield* Effect.fail(problem(404, "Document not found."));
    return row;
  });
}

export function getDocument(member: Member, reference: string) {
  return Effect.flatMap(Database, (db) => Effect.map(readDocument(db, member, reference), docDto));
}

export function listDocuments(member: Member, reference: string, query: typeof DocumentQuery.Type) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const issue = yield* readIssue(db, member, reference);
    const rows = yield* documents(db, member).where(
      and(
        docScope(member),
        eq(issueDocuments.issueId, issue.id),
        query.type ? eq(issueDocuments.type, query.type) : undefined,
        query.includeArchived ? undefined : isNull(issueDocuments.archivedAt),
      ),
    );
    const key = ({ doc }: (typeof rows)[number]) =>
      `${String(Number.MAX_SAFE_INTEGER - doc.createdAt.valueOf()).padStart(16, "0")}:${doc.id}`;
    const result = yield* page(
      rows.sort(byKey(key)),
      {
        org: member.organisationId,
        documents: issue.id,
        type: query.type,
        includeArchived: Boolean(query.includeArchived),
      },
      key,
      query.limit,
      query.cursor,
    );
    return {
      ...result,
      items: result.items.map((row) => {
        const dto = docDto(row);
        return { ...dto, content: undefined };
      }),
    };
  });
}

function revisionRows(db: Db, member: Member, documentId: string, number?: number) {
  return db
    .select({ rev: documentRevisions, author: users })
    .from(documentRevisions)
    .innerJoin(
      users,
      and(
        eq(users.id, documentRevisions.authorId),
        eq(users.organisationId, member.organisationId),
      ),
    )
    .where(
      and(
        eq(documentRevisions.organisationId, member.organisationId),
        eq(documentRevisions.documentId, documentId),
        number === undefined ? undefined : eq(documentRevisions.number, number),
      ),
    );
}

const revisionDto = ({ rev, author }: { rev: Rev; author: Member }) => ({
  number: rev.number,
  title: rev.title,
  authorUserId: author.id,
  author: person(author)!,
  createdAt: dateTime(rev.createdAt),
  note: rev.note,
  content: rev.content,
});

export function documentRevision(db: Db, member: Member, id: string, revision: number) {
  return Effect.gen(function* () {
    const [row] = yield* revisionRows(db, member, id, revision);
    if (!row) return yield* Effect.fail(problem(404, "Document revision not found."));
    return revisionDto(row);
  });
}

export function listDocumentRevisions(
  member: Member,
  reference: string,
  query: { limit?: number; cursor?: string },
) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const { doc } = yield* readDocument(db, member, reference);
    const rows = yield* revisionRows(db, member, doc.id);
    const key = ({ rev }: (typeof rows)[number]) =>
      String(2147483647 - rev.number).padStart(10, "0");
    const result = yield* page(
      rows.sort(byKey(key)),
      { org: member.organisationId, doc: doc.id },
      key,
      query.limit,
      query.cursor,
    );
    return {
      ...result,
      items: result.items.map((row) => ({ ...revisionDto(row), content: undefined })),
    };
  });
}

function writableIssue(db: Db, member: Member, reference: string) {
  return Effect.gen(function* () {
    const issue = yield* readIssue(db, member, reference);
    const { project } = yield* readProject(db, member, issue.projectId);
    if (issue.archivedAt || project.archivedAt)
      return yield* Effect.fail(
        problem(409, "Unarchive the issue and project before changing documents."),
      );
    return { issue, project };
  });
}

function recordDocumentEvent(db: Db, member: Member, doc: Doc, kind: string) {
  return Effect.gen(function* () {
    yield* db
      .update(issues)
      .set({ lastActivityAt: sql`greatest(${issues.lastActivityAt}, now())` })
      .where(and(eq(issues.organisationId, member.organisationId), eq(issues.id, doc.issueId)));
    yield* addActivity(db, member, doc.issueId, kind, {
      documentId: doc.id,
      reference: doc.reference,
      revision: doc.currentRevision,
    });
  });
}

export function createDocument(
  member: Member,
  reference: string,
  input: typeof CreateDocument.Type,
) {
  return Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockIssues(tx, member);
        const { issue, project } = yield* writableIssue(tx, member, reference);
        yield* localMedia(tx, member, issue.id, input.content, true);
        const [counter] = yield* tx
          .insert(documentCounters)
          .values({ organisationId: member.organisationId, projectId: project.id, number: 1 })
          .onConflictDoUpdate({
            target: [documentCounters.organisationId, documentCounters.projectId],
            set: { number: sql`${documentCounters.number} + 1` },
            setWhere: sql`${documentCounters.number} < 2147483647`,
          })
          .returning();
        if (!counter)
          return yield* Effect.fail(
            problem(409, "The project has reached its document number limit."),
          );
        const [doc] = yield* tx
          .insert(issueDocuments)
          .values({
            organisationId: member.organisationId,
            issueId: issue.id,
            originalProjectId: project.id,
            reference: `${project.key}-DOC-${counter.number}`,
            type: input.type,
            subtype: input.subtype,
            authorId: member.id,
          })
          .returning();
        if (!doc) return yield* Effect.fail(problem(500, "The document could not be created."));
        const [rev] = yield* tx
          .insert(documentRevisions)
          .values({
            organisationId: member.organisationId,
            documentId: doc.id,
            number: 1,
            title: input.title,
            content: input.content,
            note: input.note,
            authorId: member.id,
          })
          .returning();
        yield* recordDocumentEvent(tx, member, doc, "document.created");
        return docDto({ doc, rev: rev!, author: member });
      }),
    ),
  );
}

export function changeDocument(
  member: Member,
  reference: string,
  change: typeof EditDocument.Type | boolean,
) {
  return Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockIssues(tx, member);
        const { doc, rev, author } = yield* readDocument(tx, member, reference);
        yield* writableIssue(tx, member, doc.issueId);
        if (typeof change === "boolean") {
          if (change === Boolean(doc.archivedAt)) return docDto({ doc, rev, author });
          const [saved] = yield* tx
            .update(issueDocuments)
            .set({
              archivedAt: change ? new Date() : null,
              updatedAt: sql`now()`,
            })
            .where(docScope(member, doc.id))
            .returning();
          yield* recordDocumentEvent(
            tx,
            member,
            saved!,
            change ? "document.archived" : "document.unarchived",
          );
          return docDto({ doc: saved!, rev, author });
        }
        if (doc.archivedAt || doc.currentRevision !== change.baseRevision) {
          return yield* Effect.fail(
            problem(409, "The document is archived or its revision is stale.", doc.currentRevision),
          );
        }
        const title = change.title ?? rev.title;
        const content = change.content ?? rev.content;
        if (title === rev.title && content === rev.content) return docDto({ doc, rev, author });
        yield* localMedia(tx, member, doc.issueId, content, true);
        const number = doc.currentRevision + 1;
        const [saved] = yield* tx
          .update(issueDocuments)
          .set({ currentRevision: number, updatedAt: sql`now()` })
          .where(docScope(member, doc.id))
          .returning();
        const [revision] = yield* tx
          .insert(documentRevisions)
          .values({
            organisationId: member.organisationId,
            documentId: doc.id,
            number,
            title,
            content,
            note: change.note,
            authorId: member.id,
          })
          .returning();
        yield* recordDocumentEvent(tx, member, saved!, "document.revised");
        return docDto({ doc: saved!, rev: revision!, author });
      }),
    ),
  );
}

const params = { reference: Ref };
export const documentGroup = HttpApiGroup.make("documents")
  .add(
    HttpApiEndpoint.get("list", "/issues/:reference/documents", {
      params,
      query: DocumentQuery,
      success: pageOf(Metadata),
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("create", "/issues/:reference/documents", {
      params,
      payload: CreateDocument,
      success: Document.pipe(HttpApiSchema.status(201)),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("document", "/documents/:reference", {
      params,
      success: Document,
      error: endpointErrors,
    }),
    HttpApiEndpoint.put("edit", "/documents/:reference", {
      params,
      payload: EditDocument,
      success: Document,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("archive", "/documents/:reference/archive", {
      params,
      success: Document,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("restore", "/documents/:reference/unarchive", {
      params,
      success: Document,
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("revisions", "/documents/:reference/revisions", {
      params,
      query: PageQuery,
      success: pageOf(RevisionMetadata),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("revision", "/documents/:reference/revisions/:number", {
      params: { ...params, number: Version },
      success: Revision,
      error: endpointErrors,
    }),
  )
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const documentHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof documentGroup>,
) =>
  handlers
    .handle("list", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => listDocuments(member, params.reference, query)),
      ),
    )
    .handle("create", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          createDocument(member, params.reference, payload),
        ),
      ),
    )
    .handle("document", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => getDocument(member, params.reference))),
    )
    .handle("edit", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          changeDocument(member, params.reference, payload),
        ),
      ),
    )
    .handle("archive", ({ params }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => changeDocument(member, params.reference, true)),
      ),
    )
    .handle("restore", ({ params }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => changeDocument(member, params.reference, false)),
      ),
    )
    .handle("revisions", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          listDocumentRevisions(member, params.reference, query),
        ),
      ),
    )
    .handle("revision", ({ params }) =>
      handleDb(
        Effect.gen(function* () {
          const db = yield* Database;
          const member = yield* CurrentMember;
          const { doc } = yield* readDocument(db, member, params.reference);
          return yield* documentRevision(db, member, doc.id, params.number);
        }),
      ),
    );
