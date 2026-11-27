import { Schema } from "effect";
import { OptionalRelay, RelayedCrew } from "./crew";
import { Version } from "./initiative-contract";
import { Attachment } from "./media-contract";
import { Todo } from "./todo-model";
import { Label, Person, StateType } from "./projects";
import {
  DateOnly,
  Line,
  NullableString,
  OptionalString,
  pageOf,
  PageQuery,
  Paragraph,
  Priority,
  Ref,
  Timestamp,
  Uuid,
} from "./validation";

export const Mention = Schema.Struct({
  userId: Uuid,
  status: Schema.Literals(["canRead", "notMember"]),
  displayName: NullableString,
  role: NullableString,
  reason: NullableString,
});
const IssueType = Schema.Literals(["issue", "tracker"]);
export const StateSummary = Schema.Struct({ id: Uuid, type: StateType, name: Line(120) }).annotate({
  identifier: "StateSummary",
});
export const ActivityActor = Schema.Union([
  Schema.Struct({
    type: Schema.Literals(["human", "agent"]),
    person: Person,
    userId: Uuid,
    label: Schema.Null,
  }),
  Schema.Struct({
    type: Schema.Literal("integration"),
    person: Schema.Null,
    userId: Schema.Null,
    label: Schema.String,
  }),
]).annotate({ identifier: "ActivityActor" });
export const ActivityHead = Schema.Struct({
  kind: Schema.String,
  occurredAt: Timestamp,
  actor: ActivityActor,
  data: Schema.Record(Schema.String, Schema.Unknown),
}).annotate({ identifier: "ActivityHead" });
export const CycleSummary = Schema.Struct({
  id: Uuid,
  number: Schema.Int,
  state: Schema.Literals(["current", "upcoming", "closed"]),
}).annotate({ identifier: "CycleSummary" });
const Progress = Schema.Struct({
  total: Schema.Int,
  done: Schema.Int,
  canceled: Schema.Int,
  duplicate: Schema.Int,
  sample: Schema.Array(Schema.Struct({ id: Uuid, stateType: StateType })),
  hasMore: Schema.Boolean,
});
export const IssueFields = Schema.Struct({
  id: Uuid,
  identifier: Ref,
  projectId: Uuid,
  projectKey: Ref,
  type: IssueType,
  title: Line(200),
  summary: Schema.NullOr(Line(300)),
  state: StateSummary,
  assignee: Schema.NullOr(Person),
  readyForAgents: Schema.Boolean,
  owner: Schema.NullOr(Person),
  priority: Priority,
  estimate: Schema.NullOr(Schema.Finite),
  dueDate: Schema.NullOr(DateOnly),
  labels: Schema.Array(Label),
  milestoneId: Schema.NullOr(Uuid),
  parentId: Schema.NullOr(Uuid),
  blocked: Schema.Boolean,
  archivedAt: Schema.NullOr(Timestamp),
  createdAt: Timestamp,
  updatedAt: Timestamp,
  version: Version,
  progress: Schema.NullOr(Progress),
  commentCount: Schema.Int,
  todoCounts: Schema.Struct({ total: Schema.Int, done: Schema.Int }),
  lastActivityAt: Timestamp,
  lastActivity: Schema.NullOr(ActivityHead),
  relayedCrew: Schema.NullOr(RelayedCrew),
  provenance: Schema.Null,
  cycle: Schema.NullOr(CycleSummary),
  cycleId: Schema.NullOr(Uuid),
  pullRequestSummary: Schema.NullOr(
    Schema.Struct({
      total: Schema.Int,
      open: Schema.Int,
      closed: Schema.Int,
      merged: Schema.Int,
      closingPending: Schema.Int,
      checksState: Schema.Null,
      lastObservedAt: Timestamp,
    }),
  ),
}).annotate({ identifier: "IssueFields" });
export const IssueReference = Schema.Struct({
  id: Uuid,
  identifier: Ref,
  title: Line(200),
  summary: NullableString,
  type: IssueType,
  state: StateSummary,
  archived: Schema.Boolean,
}).annotate({ identifier: "IssueReference" });
export const IssueDetail = Schema.Struct({
  ...IssueFields.fields,
  description: NullableString,
  mentions: Schema.Array(Mention),
  todos: Schema.Array(Todo),
  createdBy: Schema.NullOr(Person),
  startedAt: Schema.NullOr(Timestamp),
  completedAt: Schema.NullOr(Timestamp),
  canceledAt: Schema.NullOr(Timestamp),
  archivedBy: Schema.NullOr(Person),
  parent: Schema.NullOr(IssueReference),
  duplicateOf: Schema.NullOr(IssueReference),
  children: Schema.NullOr(pageOf(IssueFields)),
  relations: Schema.Null,
}).annotate({ identifier: "IssueDetail" });
const editable = {
  relayedCrew: OptionalRelay,
  type: Schema.optional(IssueType),
  description: Schema.optional(Schema.NullOr(Paragraph(50_000))),
  stateId: Schema.optional(Uuid),
  assigneeId: Schema.optional(Schema.NullOr(Uuid)),
  readyForAgents: Schema.optional(Schema.Boolean),
  ownerId: Schema.optional(Schema.NullOr(Uuid)),
  priority: Schema.optional(Priority),
  estimate: Schema.optional(Schema.NullOr(Schema.Finite.check(Schema.isGreaterThanOrEqualTo(0)))),
  dueDate: Schema.optional(Schema.NullOr(DateOnly)),
  labelIds: Schema.optional(Schema.Array(Uuid).check(Schema.isMaxLength(100))),
  parentId: Schema.optional(Schema.NullOr(Ref)),
  milestoneId: Schema.optional(Schema.NullOr(Uuid)),
  cycleId: Schema.optional(Schema.NullOr(Uuid)),
};
const referenceList = Schema.optional(Schema.Array(Ref).check(Schema.isMaxLength(100)));
export const RelationList = Schema.Struct({
  blocks: referenceList,
  blockedBy: referenceList,
  relatedTo: referenceList,
});
const DuplicateTarget = Schema.optional(Schema.NullOr(Ref));
export const Relation = Schema.Struct({
  ...IssueFields.fields,
  kind: Schema.Literals(["blocks", "blockedBy", "relatedTo", "duplicateOf", "duplicatedBy"]),
}).annotate({ identifier: "IssueRelation" });

export const NewIssue = Schema.Struct({
  relations: Schema.optional(
    Schema.Struct({ ...RelationList.fields, duplicateOf: DuplicateTarget }),
  ),
  projectId: Uuid,
  ...editable,
  title: Line(200),
  summary: Line(300),
});
export const EditIssue = Schema.Struct({
  relations: Schema.optional(
    Schema.Struct({
      add: Schema.optional(Schema.NullOr(RelationList)),
      remove: Schema.optional(Schema.NullOr(RelationList)),
      duplicateOf: DuplicateTarget,
    }),
  ),
  /** Confirm filling empty assignees on the tracker's existing direct children. */
  assignUnassignedChildren: Schema.optional(Schema.Boolean),
  /** Confirm moving the tracker's open direct issue children to Backlog or Todo. */
  moveOpenChildren: Schema.optional(Schema.Boolean),
  version: Version,
  ...editable,
  title: Schema.optional(Line(200)),
  summary: Schema.optional(Line(300)),
});

export const IssueFilter = {
  project: Schema.optional(Ref),
  stateId: Schema.optional(Uuid),
  // HttpApi normalises single query values to one-element arrays for array schemas.
  stateType: Schema.optional(Schema.Array(StateType)),
  state: Schema.optional(Schema.Array(Ref)),
  priority: Schema.optional(Schema.Array(Priority)),
  assignee: Schema.optional(Schema.Array(Ref)),
  assigneeKind: Schema.optional(Schema.Literals(["human", "agent"])),
  owner: Schema.optional(Schema.Array(Ref)),
  label: Schema.optional(Schema.Array(Uuid)),
  creator: Schema.optional(Schema.Array(Ref)),
  projectIds: Schema.optional(Schema.Array(Uuid)),
  milestone: Schema.optional(Schema.Array(Ref)),
  cycle: Schema.optional(Schema.Array(Ref)),
  subscribed: Schema.optional(Schema.Boolean),
  readyForAgents: Schema.optional(Schema.Boolean),
  updatedSince: Schema.optional(Schema.DateTimeUtcFromString),
  completedSince: Schema.optional(Schema.DateTimeUtcFromString),
  activitySince: Schema.optional(Schema.DateTimeUtcFromString),
  type: Schema.optional(IssueType),
  parent: Schema.optional(Ref),
  q: Schema.optional(Paragraph(200)),
  includeArchived: Schema.optional(Schema.Boolean),
  order: Schema.optional(Schema.Literals(["created", "priority", "manual", "activity"])),
  due: Schema.optional(Schema.Literals(["overdue", "thisWeek", "none"])),
};
export const IssueQuery = Schema.Struct({
  ...IssueFilter,
  parentFor: Schema.optional(Ref),
  ...PageQuery,
});
export const GroupQuery = Schema.Struct({
  ...IssueFilter,
  groupBy: Schema.Literals(["state", "stateType"]),
  limitPerGroup: Schema.optional(Schema.Int.check(Schema.isBetween({ minimum: 0, maximum: 25 }))),
});
export const IssueGroups = Schema.Struct({
  groupBy: Schema.Literals(["state", "stateType"]),
  groups: Schema.Array(
    Schema.Struct({
      stateId: Schema.NullOr(Uuid),
      stateType: StateType,
      count: Schema.Int,
      items: Schema.Array(IssueFields),
      nextCursor: NullableString,
    }),
  ),
}).annotate({ identifier: "IssueGroups" });
export const Comment = Schema.Struct({
  id: Uuid,
  issueId: Uuid,
  parentCommentId: Schema.NullOr(Uuid),
  body: Paragraph(50_000),
  version: Version,
  actualAuthor: Person,
  createdAt: Timestamp,
  displayCreatedAt: Timestamp,
  editedAt: Schema.NullOr(Timestamp),
  deletedAt: Schema.NullOr(Timestamp),
  resolved: Schema.Boolean,
  resolvedBy: Schema.NullOr(Person),
  resolvedAt: Schema.NullOr(Timestamp),
  replyCount: Schema.Int,
  hasReplies: Schema.Boolean,
  provenance: Schema.Null,
  relayedCrew: Schema.NullOr(RelayedCrew),
  media: Schema.Array(Attachment),
  mentions: Schema.Array(Mention),
}).annotate({ identifier: "Comment" });
export const CommentInput = Schema.Struct({
  relayedCrew: OptionalRelay,
  body: Paragraph(50_000).check(Schema.isPattern(/\S/u)),
  parentCommentId: Schema.optional(Schema.NullOr(Uuid)),
});
export const EditComment = Schema.Struct({
  relayedCrew: OptionalRelay,
  body: CommentInput.fields.body,
  version: Schema.optional(Version),
});
export const RemoveComment = Schema.Struct({
  version: Schema.optional(Version),
  relayedCrew: OptionalRelay,
});
export const ResolveComment = Schema.Struct({
  relayedCrew: OptionalRelay,
  resolved: Schema.Boolean,
  version: Schema.optional(Version),
});
export const CommentLocation = Schema.Struct({
  issueId: Uuid,
  requestedCommentId: Uuid,
  root: Comment,
  replies: Schema.Array(Comment),
  beforeCursor: NullableString,
  afterCursor: NullableString,
});
export const ActivityQuery = Schema.Struct({ ...PageQuery, before: OptionalString });

export const ActivityEntry = Schema.Struct({
  id: Uuid,
  ...ActivityHead.fields,
  displayAt: Timestamp,
  comment: Schema.NullOr(Comment),
}).annotate({ identifier: "ActivityEntry" });
export const ActivityPage = Schema.Struct({
  ...pageOf(ActivityEntry).fields,
  references: Schema.Struct({
    decisions: Schema.Array(
      Schema.Struct({
        id: Uuid,
        title: Line(200),
        status: Schema.Literals(["proposed", "accepted", "rejected", "superseded"]),
      }),
    ),
  }),
});
