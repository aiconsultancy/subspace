import { and, eq, isNull, sql } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { HttpApiEndpoint, HttpApiGroup, HttpApiSchema } from "effect/http-api";
import { recordAudit } from "./audit";
import { Database } from "./db";
import { documentRevision, readDocument, ReviewKind } from "./documents";
import { CurrentMember, type Member } from "./identity";
import { Version } from "./initiative-contract";
import { addActivity, lockIssues, readIssue } from "./issues";
import { BearerAuth, handleDb, ValidationErrors, versionedApi } from "./middleware";
import { byKey, page } from "./paging";
import { endpointErrors, problem } from "./problems";
import { person, Person, readProject } from "./projects";
import {
  issueReviews,
  issues,
  projectReviewRequirements,
  projects,
  reviewReasoning,
  reviewVerdicts,
  users,
} from "./schema";
import {
  dateTime,
  Line,
  NullableString,
  PageQuery,
  pageOf,
  Paragraph,
  Ref,
  Timestamp,
  Uuid,
} from "./validation";

const Reasoning = Schema.Literals(reviewReasoning);
const Verdict = Schema.Literals(reviewVerdicts);
const Model = Line(100).check(Schema.isPattern(/[\p{L}\p{N}]/u));
const Requirement = Schema.Struct({ model: Model, reasoning: Schema.NullOr(Reasoning) });
const Requirements = Schema.Array(Requirement).check(Schema.isMaxLength(10));
const Kinds = Schema.Struct({
  spec: Requirements,
  requirements: Requirements,
  design: Requirements,
  general: Requirements,
});
const RequirementInput = Schema.Array(
  Schema.Struct({
    model: Model,
    reasoning: Schema.optional(Schema.NullOr(Reasoning)),
  }),
).check(Schema.isMaxLength(10));
export const RequirementsInput = Schema.Struct({
  version: Version,
  kinds: Schema.Struct({
    spec: Schema.optional(RequirementInput),
    requirements: Schema.optional(RequirementInput),
    design: Schema.optional(RequirementInput),
    general: Schema.optional(RequirementInput),
  }),
});
const ProjectRequirements = Schema.Struct({ project: Ref, version: Version, kinds: Kinds });
const target = {
  kind: Schema.optional(ReviewKind),
  revision: Schema.optional(Line(120)),
  document: Schema.optional(Ref),
  documentRevision: Schema.optional(Version),
};
export const ListReviewQuery = Schema.Struct({ ...PageQuery, ...target });
export const RecordReview = Schema.Struct({
  ...target,
  model: Model,
  reasoning: Schema.optional(Schema.NullOr(Reasoning)),
  verdict: Verdict,
  subject: Schema.optional(Line(200)),
  notes: Schema.optional(Schema.NullOr(Paragraph(10_000))),
});
export const IssueReviewStatus = Schema.Struct({
  kind: ReviewKind,
  subject: Line(200),
  revision: Line(120),
});
const ReviewStatus = Schema.Struct({
  kind: ReviewKind,
  revision: Schema.String,
  requirements: Schema.Array(
    Schema.Struct({
      ...Requirement.fields,
      status: Schema.Literals(["met", "missing"]),
      reviewId: Schema.NullOr(Uuid),
    }),
  ),
  allMet: Schema.Boolean,
  documentId: Schema.NullOr(Uuid),
  documentRevision: Schema.NullOr(Version),
  subject: NullableString,
});
const Review = Schema.Struct({
  id: Uuid,
  kind: ReviewKind,
  model: Model,
  reasoning: Schema.NullOr(Reasoning),
  verdict: Verdict,
  subject: NullableString,
  revision: NullableString,
  documentId: Schema.NullOr(Uuid),
  documentRevision: Schema.NullOr(Version),
  notes: NullableString,
  recordedBy: Person,
  createdAt: Timestamp,
});

type Db = (typeof Database)["Service"];
type Row = typeof issueReviews.$inferSelect;
type RequirementSet = typeof Kinds.Type;
type RequirementEntry = typeof Requirement.Type;
type ReviewKindValue = typeof ReviewKind.Type;

const emptyKinds = (): RequirementSet => ({ spec: [], requirements: [], design: [], general: [] });

function requirements(db: Db, member: Member, projectId: string) {
  return Effect.map(
    db
      .select({ kinds: projectReviewRequirements.kinds })
      .from(projectReviewRequirements)
      .where(
        and(
          eq(projectReviewRequirements.organisationId, member.organisationId),
          eq(projectReviewRequirements.projectId, projectId),
        ),
      ),
    (rows) => rows[0]?.kinds ?? emptyKinds(),
  );
}

export function getRequirements(member: Member, reference: string) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const { project } = yield* readProject(db, member, reference);
    return {
      project: project.key,
      version: project.version,
      kinds: yield* requirements(db, member, project.id),
    };
  });
}

export function setRequirements(
  member: Member,
  reference: string,
  input: typeof RequirementsInput.Type,
) {
  return Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockIssues(tx, member);
        const { project } = yield* readProject(tx, member, reference);
        if (member.role !== "admin" && member.id !== project.leadId)
          return yield* Effect.fail(
            problem(403, "Review requirements can be set by the project lead or an administrator."),
          );
        if (project.archivedAt || input.version !== project.version)
          return yield* Effect.fail(
            problem(409, "The project is archived or its version is stale.", project.version),
          );
        const kinds: typeof projectReviewRequirements.$inferSelect.kinds = {
          spec: [],
          requirements: [],
          design: [],
          general: [],
        };
        for (const kind of ["spec", "requirements", "design", "general"] as const) {
          kinds[kind] =
            input.kinds[kind]?.map(({ model, reasoning }) => ({
              model,
              reasoning: reasoning ?? null,
            })) ?? [];
        }
        const version = project.version + 1;
        yield* tx
          .update(projects)
          .set({ version, updatedAt: sql`now()` })
          .where(
            and(eq(projects.organisationId, member.organisationId), eq(projects.id, project.id)),
          );
        yield* tx
          .insert(projectReviewRequirements)
          .values({ organisationId: member.organisationId, projectId: project.id, kinds })
          .onConflictDoUpdate({
            target: [projectReviewRequirements.organisationId, projectReviewRequirements.projectId],
            set: { kinds },
          });
        yield* recordAudit(tx, member, {
          action: "projectReviewRequirementsUpdated",
          projectId: project.id,
          entityType: "Project",
          entityId: project.id,
          summary: "Changed project review requirements.",
          changes: { version },
        });
        return { project: project.key, version, kinds };
      }),
    ),
  );
}

const reviewScope = (member: Member, issueId: string) =>
  and(eq(issueReviews.organisationId, member.organisationId), eq(issueReviews.issueId, issueId));

function reviewDto(row: Row, author: Member) {
  const {
    id,
    kind,
    model,
    reasoning,
    verdict,
    subject,
    revision,
    documentId,
    documentRevision,
    notes,
  } = row;
  return {
    id,
    kind,
    model,
    reasoning,
    verdict,
    subject,
    revision,
    documentId,
    documentRevision,
    notes,
    recordedBy: person(author)!,
    createdAt: dateTime(row.createdAt),
  };
}

export function listReviews(member: Member, reference: string, query: typeof ListReviewQuery.Type) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const issue = yield* readIssue(db, member, reference);
    const document = query.document ? (yield* readDocument(db, member, query.document)).doc : null;
    if (document && document.issueId !== issue.id)
      return yield* Effect.fail(problem(404, "Document not found for this issue."));
    if (query.documentRevision !== undefined && !document)
      return yield* Effect.fail(problem(400, "Supply a document for its revision."));
    const rows = yield* db
      .select({ row: issueReviews, author: users })
      .from(issueReviews)
      .innerJoin(
        users,
        and(
          eq(users.organisationId, member.organisationId),
          eq(users.id, issueReviews.recordedById),
        ),
      )
      .where(
        and(
          reviewScope(member, issue.id),
          query.kind ? eq(issueReviews.kind, query.kind) : undefined,
          query.revision ? eq(issueReviews.revision, query.revision) : undefined,
          document ? eq(issueReviews.documentId, document.id) : undefined,
          query.documentRevision === undefined
            ? undefined
            : eq(issueReviews.documentRevision, query.documentRevision),
        ),
      );
    const key = ({ row }: (typeof rows)[number]) =>
      `${String(Number.MAX_SAFE_INTEGER - row.createdAt.valueOf()).padStart(16, "0")}:${row.id}`;
    const result = yield* page(
      rows.sort(byKey(key)),
      {
        org: member.organisationId,
        issue: issue.id,
        kind: query.kind,
        revision: query.revision,
        document: document?.id,
        documentRevision: query.documentRevision,
      },
      key,
      query.limit,
      query.cursor,
    );
    return { ...result, items: result.items.map(({ row, author }) => reviewDto(row, author)) };
  });
}

export function recordReview(member: Member, reference: string, input: typeof RecordReview.Type) {
  return Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockIssues(tx, member);
        const issue = yield* readIssue(tx, member, reference);
        const { project } = yield* readProject(tx, member, issue.projectId);
        if (issue.archivedAt || project.archivedAt)
          return yield* Effect.fail(
            problem(409, "Unarchive the issue and project before recording reviews."),
          );
        const { document, documentRevision: number, ...fields } = input;
        const doc = document ? (yield* readDocument(tx, member, document)).doc : null;
        if (doc && doc.issueId !== issue.id)
          return yield* Effect.fail(problem(404, "Document not found for this issue."));
        if (
          (!doc && number !== undefined) ||
          (doc && (fields.revision || fields.subject || (fields.kind && fields.kind !== doc.type)))
        )
          return yield* Effect.fail(
            problem(
              400,
              "Use either a document revision or an external review subject and revision.",
            ),
          );
        if (!doc && Boolean(fields.subject) !== Boolean(fields.revision))
          return yield* Effect.fail(
            problem(400, "Provide both external review subject and revision."),
          );
        if (doc?.archivedAt)
          return yield* Effect.fail(
            problem(409, "Unarchive the document before recording reviews."),
          );
        const revision = doc ? (number ?? doc.currentRevision) : null;
        if (doc && revision !== null) yield* documentRevision(tx, member, doc.id, revision);
        const [row] = yield* tx
          .insert(issueReviews)
          .values({
            ...fields,
            kind: doc?.type ?? fields.kind ?? "general",
            organisationId: member.organisationId,
            issueId: issue.id,
            documentId: doc?.id,
            documentRevision: revision,
            recordedById: member.id,
          })
          .returning();
        if (!row) return yield* Effect.fail(problem(500, "The review could not be recorded."));
        yield* tx
          .update(issues)
          .set({ lastActivityAt: sql`greatest(${issues.lastActivityAt}, now())` })
          .where(and(eq(issues.organisationId, member.organisationId), eq(issues.id, issue.id)));
        yield* addActivity(tx, member, issue.id, "review.recorded", {
          reviewId: row.id,
          verdict: row.verdict,
        });
        return reviewDto(row, member);
      }),
    ),
  );
}

const modelTokens = (model: string) =>
  model
    .toLowerCase()
    .split(/[-_.\s]+/u)
    .filter(Boolean);

function matches(requirement: RequirementEntry, review: Row) {
  const required = modelTokens(requirement.model);
  const actual = modelTokens(review.model);
  return (
    actual.some((_, start) =>
      required.every((token, offset) => actual[start + offset] === token),
    ) &&
    (requirement.reasoning === null ||
      (review.reasoning !== null &&
        reviewReasoning.indexOf(review.reasoning) >=
          reviewReasoning.indexOf(requirement.reasoning)))
  );
}

function reviewStatus(
  db: Db,
  member: Member,
  issue: typeof issues.$inferSelect,
  target: {
    kind: ReviewKindValue;
    subject: string | null;
    revision: string;
    documentId: string | null;
    documentRevision: number | null;
  },
) {
  return Effect.gen(function* () {
    const required = (yield* requirements(db, member, issue.projectId))[target.kind];
    const rows = yield* db
      .select()
      .from(issueReviews)
      .where(
        and(
          reviewScope(member, issue.id),
          eq(issueReviews.kind, target.kind),
          eq(issueReviews.verdict, "approved"),
          target.documentId && target.documentRevision !== null
            ? and(
                eq(issueReviews.documentId, target.documentId),
                eq(issueReviews.documentRevision, target.documentRevision),
              )
            : and(
                isNull(issueReviews.documentId),
                eq(issueReviews.subject, target.subject!),
                eq(issueReviews.revision, target.revision),
              ),
        ),
      )
      .orderBy(issueReviews.createdAt, issueReviews.id);
    const used = new Set<string>();
    const status = required.map((requirement) => {
      const review = rows.find((row) => !used.has(row.id) && matches(requirement, row));
      if (review) used.add(review.id);
      return {
        ...requirement,
        status: review ? ("met" as const) : ("missing" as const),
        reviewId: review?.id ?? null,
      };
    });
    return {
      ...target,
      requirements: status,
      allMet: status.every(({ status }) => status === "met"),
    };
  });
}

export function issueReviewStatus(
  member: Member,
  reference: string,
  query: typeof IssueReviewStatus.Type,
) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const issue = yield* readIssue(db, member, reference);
    return yield* reviewStatus(db, member, issue, {
      ...query,
      documentId: null,
      documentRevision: null,
    });
  });
}

export function documentReviewStatus(member: Member, reference: string, requested?: number) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const { doc } = yield* readDocument(db, member, reference);
    const issue = yield* readIssue(db, member, doc.issueId);
    const revision = requested ?? doc.currentRevision;
    if (requested !== undefined) yield* documentRevision(db, member, doc.id, revision);
    return yield* reviewStatus(db, member, issue, {
      kind: doc.type,
      revision: String(revision),
      subject: null,
      documentId: doc.id,
      documentRevision: revision,
    });
  });
}

const params = { reference: Ref };
export const reviewGroup = HttpApiGroup.make("reviews")
  .add(
    HttpApiEndpoint.get("requirements", "/projects/:reference/review-requirements", {
      params,
      success: ProjectRequirements,
      error: endpointErrors,
    }),
    HttpApiEndpoint.put("setRequirements", "/projects/:reference/review-requirements", {
      params,
      payload: RequirementsInput,
      success: ProjectRequirements,
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("reviews", "/issues/:reference/reviews", {
      params,
      query: ListReviewQuery,
      success: pageOf(Review),
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("review", "/issues/:reference/reviews", {
      params,
      payload: RecordReview,
      success: Review.pipe(HttpApiSchema.status(201)),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("issueStatus", "/issues/:reference/review-status", {
      params,
      query: IssueReviewStatus,
      success: ReviewStatus,
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("documentStatus", "/documents/:reference/review-status", {
      params,
      query: { revision: Schema.optional(Version) },
      success: ReviewStatus,
      error: endpointErrors,
    }),
  )
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const reviewHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof reviewGroup>,
) =>
  handlers
    .handle("requirements", ({ params }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => getRequirements(member, params.reference)),
      ),
    )
    .handle("setRequirements", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          setRequirements(member, params.reference, payload),
        ),
      ),
    )
    .handle("reviews", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => listReviews(member, params.reference, query)),
      ),
    )
    .handle("review", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => recordReview(member, params.reference, payload)),
      ),
    )
    .handle("issueStatus", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          issueReviewStatus(member, params.reference, query),
        ),
      ),
    )
    .handle("documentStatus", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          documentReviewStatus(member, params.reference, query.revision),
        ),
      ),
    );
