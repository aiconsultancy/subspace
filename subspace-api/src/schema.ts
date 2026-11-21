import {
  bigint,
  boolean,
  check,
  date,
  doublePrecision,
  foreignKey,
  index,
  integer,
  jsonb,
  pgTable,
  primaryKey,
  text,
  timestamp,
  unique,
  uniqueIndex,
  uuid,
} from "drizzle-orm/pg-core";
import { sql } from "drizzle-orm";
import { issuePriorityOrder, issueTimeOrder } from "./issue-order";

const createdAt = () => timestamp("created_at", { withTimezone: true }).defaultNow().notNull();
const updatedAt = () => timestamp("updated_at", { withTimezone: true }).defaultNow().notNull();
const id = () => uuid().defaultRandom().primaryKey();
const organisationId = () =>
  uuid("organisation_id")
    .notNull()
    .references(() => organisations.id, { onDelete: "cascade" });

export const organisations = pgTable(
  "organisations",
  {
    id: id(),
    provider: text({ enum: ["workos", "entra", "dev"] }).notNull(),
    externalId: text("external_id").notNull(),
  },
  (table) => [unique().on(table.provider, table.externalId)],
);

export const fleets = pgTable(
  "fleets",
  {
    id: id(),
    organisationId: organisationId(),
    slug: text().notNull(),
    name: text().notNull(),
    ircHost: text("irc_host"),
    ircPort: integer("irc_port"),
    ircTls: boolean("irc_tls"),
    ircWebsocketUrl: text("irc_websocket_url"),
    legacyRearAdmiralName: text("rear_admiral_name"),
    legacyCaptainNames: jsonb("captain_names").$type<readonly string[]>(),
    vmHost: text("vm_host"),
    version: integer().notNull().default(1),
    createdAt: createdAt(),
    updatedAt: updatedAt(),
  },
  (table) => [
    unique("fleets_organisation_slug").on(table.organisationId, table.slug),
    unique().on(table.organisationId, table.id),
    check("fleets_irc_port", sql`${table.ircPort} between 1 and 65535`),
  ],
);

export const ships = pgTable(
  "ships",
  {
    id: id(),
    organisationId: organisationId(),
    fleetId: uuid("fleet_id").notNull(),
    name: text().notNull(),
    version: integer().notNull().default(1),
    createdAt: createdAt(),
    updatedAt: updatedAt(),
  },
  (table) => [
    unique("ships_scope_id").on(table.organisationId, table.fleetId, table.id),
    uniqueIndex("ships_fleet_name").on(table.fleetId, sql`lower(${table.name})`),
    foreignKey({
      columns: [table.organisationId, table.fleetId],
      foreignColumns: [fleets.organisationId, fleets.id],
    }),
  ],
);

export const users = pgTable(
  "users",
  {
    id: id(),
    organisationId: organisationId(),
    subject: text().notNull(),
    email: text(),
    displayName: text("display_name").notNull(),
    role: text({ enum: ["admin", "member"] })
      .default("member")
      .notNull(),
    active: boolean().default(true).notNull(),
    agentRank: text("agent_rank", { enum: ["rearAdmiral", "captain", "crew"] }),
    agentFleet: text("agent_fleet"),
    agentShip: text("agent_ship"),
    agentFleetId: uuid("agent_fleet_id"),
    agentShipId: uuid("agent_ship_id"),
    agentModel: text("agent_model"),
    registeredById: uuid("registered_by_id"),
    deactivatedById: uuid("deactivated_by_id"),
    deactivatedAt: timestamp("deactivated_at", { withTimezone: true }),
    version: integer().notNull().default(1),
    createdAt: createdAt(),
    updatedAt: updatedAt(),
  },
  (table) => [
    unique().on(table.organisationId, table.subject),
    unique().on(table.organisationId, table.id),
    check("users_role", sql`${table.role} in ('admin', 'member')`),
    check(
      "agent_members",
      sql`${table.agentRank} is null or (
      ${table.role} = 'member' and ${table.agentRank} in ('rearAdmiral', 'captain', 'crew') and
      ${table.agentFleet} is not null and ${table.agentModel} is not null and ${table.registeredById} is not null
    )`,
    ),
    check(
      "agent_ship_requires_fleet",
      sql`${table.agentShipId} is null or ${table.agentFleetId} is not null`,
    ),
    foreignKey({
      columns: [table.organisationId, table.agentFleetId],
      foreignColumns: [fleets.organisationId, fleets.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.agentFleetId, table.agentShipId],
      foreignColumns: [ships.organisationId, ships.fleetId, ships.id],
    }),
    index("users_agent_fleet").on(table.agentFleetId),
    uniqueIndex("user_agent_name")
      .on(table.organisationId, sql`lower(${table.displayName})`)
      .where(sql`${table.agentRank} is not null`),
    foreignKey({
      columns: [table.organisationId, table.registeredById],
      foreignColumns: [table.organisationId, table.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.deactivatedById],
      foreignColumns: [table.organisationId, table.id],
    }),
  ],
);

export const initiatives = pgTable(
  "initiatives",
  {
    id: id(),
    organisationId: organisationId(),
    name: text().notNull(),
    channels: text().notNull().default(""),
    description: text(),
    ownerId: uuid("owner_id").notNull(),
    parentId: uuid("parent_id"),
    fleetId: uuid("fleet_id"),
    version: integer().notNull().default(1),
    status: text({ enum: ["planned", "active", "completed", "canceled"] })
      .notNull()
      .default("planned"),
    targetDate: date("target_date"),
    archived: boolean().default(false).notNull(),
    createdAt: createdAt(),
    updatedAt: updatedAt(),
  },
  (table) => [
    unique().on(table.organisationId, table.id),
    foreignKey({
      columns: [table.organisationId, table.ownerId],
      foreignColumns: [users.organisationId, users.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.fleetId],
      foreignColumns: [fleets.organisationId, fleets.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.parentId],
      foreignColumns: [table.organisationId, table.id],
    }),
    check(
      "initiatives_status",
      sql`${table.status} in ('planned', 'active', 'completed', 'canceled')`,
    ),
    check("initiatives_version", sql`${table.version} > 0`),
  ],
);

export const initiativeMemberships = pgTable(
  "initiative_memberships",
  {
    organisationId: organisationId(),
    initiativeId: uuid("initiative_id").notNull(),
    userId: uuid("user_id").notNull(),
  },
  (table) => [
    primaryKey({ columns: [table.organisationId, table.initiativeId, table.userId] }),
    foreignKey({
      columns: [table.organisationId, table.initiativeId],
      foreignColumns: [initiatives.organisationId, initiatives.id],
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.organisationId, table.userId],
      foreignColumns: [users.organisationId, users.id],
    }).onDelete("cascade"),
    index().on(table.organisationId, table.userId),
  ],
);

export const projectStatuses = [
  "backlog",
  "planned",
  "started",
  "paused",
  "completed",
  "canceled",
] as const;
export const stateTypes = [
  "triage",
  "backlog",
  "unstarted",
  "started",
  "completed",
  "canceled",
  "duplicate",
] as const;
export const issueTypes = ["issue", "tracker"] as const;

export const estimateScales = ["notUsed", "fibonacci", "exponential", "linear", "tShirt"] as const;

export const cycleStartDays = [
  "monday",
  "tuesday",
  "wednesday",
  "thursday",
  "friday",
  "saturday",
  "sunday",
] as const;
type CycleSchedule = {
  enabled: boolean;
  lengthWeeks: number;
  startDay: (typeof cycleStartDays)[number];
  cyclesAhead: number;
  carryOver: boolean;
};
export const defaultCycleSchedule: CycleSchedule = {
  enabled: false,
  lengthWeeks: 2,
  startDay: "monday",
  cyclesAhead: 2,
  carryOver: true,
};

export const projects = pgTable(
  "projects",
  {
    id: id(),
    organisationId: organisationId(),
    key: text().notNull(),
    name: text().notNull(),
    description: text(),
    leadId: uuid("lead_id").notNull(),
    createdByUserId: uuid("created_by_user_id").notNull(),
    status: text({ enum: projectStatuses }).default("backlog").notNull(),
    priority: integer().default(0).notNull(),
    health: text({ enum: ["onTrack", "atRisk", "offTrack"] }),
    estimateScale: text("estimate_scale", { enum: estimateScales }).default("notUsed").notNull(),
    startDate: date("start_date"),
    targetDate: date("target_date"),
    initiativeId: uuid("initiative_id"),
    defaultStateId: uuid("default_state_id").notNull(),
    archivedAt: timestamp("archived_at", { withTimezone: true }),
    archivedByUserId: uuid("archived_by_user_id"),
    version: integer().default(1).notNull(),
    nextIssueNumber: integer("next_issue_number").default(1).notNull(),
    lastCycleNumber: integer("last_cycle_number").default(0).notNull(),
    cycleSettings: jsonb("cycle_settings")
      .$type<CycleSchedule>()
      .default(defaultCycleSchedule)
      .notNull(),
    createdAt: createdAt(),
    updatedAt: updatedAt(),
  },
  (table) => [
    unique().on(table.organisationId, table.id),
    unique().on(table.organisationId, table.key),
    foreignKey({
      columns: [table.organisationId, table.initiativeId],
      foreignColumns: [initiatives.organisationId, initiatives.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.leadId],
      foreignColumns: [users.organisationId, users.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.createdByUserId],
      foreignColumns: [users.organisationId, users.id],
    }),
  ],
);

export const states = pgTable(
  "workflow_states",
  {
    id: id(),
    organisationId: organisationId(),
    projectId: uuid("project_id").notNull(),
    name: text().notNull(),
    type: text({ enum: stateTypes }).notNull(),
    position: integer().notNull(),
    isDefault: boolean("is_default").notNull().default(false),
  },
  (table) => [
    unique().on(table.organisationId, table.projectId, table.id),
    unique().on(table.organisationId, table.projectId, table.type, table.position),
    foreignKey({
      columns: [table.organisationId, table.projectId],
      foreignColumns: [projects.organisationId, projects.id],
    }),
  ],
);

export const labels = pgTable(
  "labels",
  {
    id: id(),
    organisationId: organisationId(),
    name: text().notNull(),
    color: text().notNull(),
    state: text({ enum: ["active", "archived"] })
      .default("active")
      .notNull(),
  },
  (table) => [
    unique().on(table.organisationId, table.id),
    unique().on(table.organisationId, table.name),
  ],
);

export const milestones = pgTable(
  "project_milestones",
  {
    id: id(),
    organisationId: organisationId(),
    projectId: uuid("project_id").notNull(),
    name: text().notNull(),
    description: text(),
    targetDate: date("target_date"),
    sortOrder: integer("sort_order").notNull().default(0),
    completedAt: timestamp("completed_at", { withTimezone: true }),
    completedById: uuid("completed_by_id"),
    version: integer().notNull().default(1),
    createdAt: createdAt(),
    updatedAt: updatedAt(),
  },
  (table) => [
    unique().on(table.organisationId, table.projectId, table.id),
    foreignKey({
      columns: [table.organisationId, table.completedById],
      foreignColumns: [users.organisationId, users.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.projectId],
      foreignColumns: [projects.organisationId, projects.id],
    }),
  ],
);

export const cycles = pgTable(
  "issue_cycles",
  {
    id: id(),
    organisationId: organisationId(),
    projectId: uuid("project_id").notNull(),
    number: integer().notNull(),
    state: text({ enum: ["current", "upcoming", "closed"] }).notNull(),
    startDate: date("start_date").notNull(),
    endDate: date("end_date").notNull(),
    finishedAt: timestamp("finished_at", { withTimezone: true }),
    finishedEarly: boolean("finished_early").default(false).notNull(),
    carriedToNumber: integer("carried_to_number"),
  },
  (table) => [
    unique().on(table.organisationId, table.projectId, table.id),
    unique().on(table.organisationId, table.projectId, table.number),
    foreignKey({
      columns: [table.organisationId, table.projectId],
      foreignColumns: [projects.organisationId, projects.id],
    }),
  ],
);

export const issues = pgTable(
  "issues",
  {
    id: id(),
    organisationId: organisationId(),
    projectId: uuid("project_id").notNull(),
    number: integer().notNull(),
    identifier: text().notNull(),
    type: text({ enum: issueTypes }).default("issue").notNull(),
    title: text().notNull(),
    summary: text().notNull(),
    description: text(),
    stateId: uuid("state_id").notNull(),
    assigneeId: uuid("assignee_id"),
    readyForAgents: boolean("ready_for_agents").default(false).notNull(),
    ownerId: uuid("owner_id"),
    createdById: uuid("created_by_id").notNull(),
    parentId: uuid("parent_id"),
    milestoneId: uuid("milestone_id"),
    cycleId: uuid("cycle_id"),
    cycleJoinedAt: timestamp("cycle_joined_at", { withTimezone: true }),
    priority: integer().default(0).notNull(),
    estimate: doublePrecision(),
    dueDate: date("due_date"),
    relayedCrew: jsonb("relayed_crew").$type<{ name: string; model: string }>(),
    version: integer().default(1).notNull(),
    archivedById: uuid("archived_by_id"),
    archivedAt: timestamp("archived_at", { withTimezone: true }),
    startedAt: timestamp("started_at", { withTimezone: true }),
    completedAt: timestamp("completed_at", { withTimezone: true }),
    canceledAt: timestamp("canceled_at", { withTimezone: true }),
    createdAt: createdAt(),
    updatedAt: updatedAt(),
    lastActivityAt: timestamp("last_activity_at", { withTimezone: true }).defaultNow().notNull(),
  },
  (table) => [
    unique().on(table.organisationId, table.id),
    unique().on(table.organisationId, table.identifier),
    index().on(table.organisationId, table.projectId),
    index("issues_created_order_idx").on(
      table.organisationId,
      issueTimeOrder(table.createdAt),
      table.id,
    ),
    index("issues_priority_order_idx").on(
      table.organisationId,
      issuePriorityOrder(table.priority),
      issueTimeOrder(table.createdAt),
      table.id,
    ),
    index("issues_activity_order_idx").on(
      table.organisationId,
      issueTimeOrder(table.lastActivityAt),
      issueTimeOrder(table.createdAt),
      table.id,
    ),
    index("issues_activity_since_idx").on(table.organisationId, table.lastActivityAt),
    index("issues_manual_order_idx").on(table.organisationId, table.number, table.id),
    // Supports the dispatcher's poll for ready, unassigned and unarchived issues.
    index("issues_ready_for_agents_idx")
      .on(table.organisationId)
      .where(
        sql`${table.readyForAgents} and ${table.assigneeId} is null and ${table.archivedAt} is null`,
      ),
    index("issues_parent_order_idx").on(
      table.organisationId,
      table.parentId,
      table.number,
      table.id,
    ),
    foreignKey({
      columns: [table.organisationId, table.projectId],
      foreignColumns: [projects.organisationId, projects.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.parentId],
      foreignColumns: [table.organisationId, table.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.projectId, table.stateId],
      foreignColumns: [states.organisationId, states.projectId, states.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.projectId, table.milestoneId],
      foreignColumns: [milestones.organisationId, milestones.projectId, milestones.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.projectId, table.cycleId],
      foreignColumns: [cycles.organisationId, cycles.projectId, cycles.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.assigneeId],
      foreignColumns: [users.organisationId, users.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.ownerId],
      foreignColumns: [users.organisationId, users.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.createdById],
      foreignColumns: [users.organisationId, users.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.archivedById],
      foreignColumns: [users.organisationId, users.id],
    }),
    check("issue_priority", sql`${table.priority} between 0 and 4`),
  ],
);

export const issueLabels = pgTable(
  "issue_labels",
  {
    organisationId: organisationId(),
    issueId: uuid("issue_id").notNull(),
    labelId: uuid("label_id").notNull(),
  },
  (table) => [
    primaryKey({ columns: [table.organisationId, table.issueId, table.labelId] }),
    foreignKey({
      columns: [table.organisationId, table.issueId],
      foreignColumns: [issues.organisationId, issues.id],
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.organisationId, table.labelId],
      foreignColumns: [labels.organisationId, labels.id],
    }),
  ],
);

export const comments = pgTable(
  "issue_comments",
  {
    id: id(),
    organisationId: organisationId(),
    issueId: uuid("issue_id").notNull(),
    parentCommentId: uuid("parent_comment_id"),
    authorId: uuid("author_id").notNull(),
    body: text().notNull(),
    relayedCrew: jsonb("relayed_crew").$type<{ name: string; model: string }>(),
    version: integer().default(1).notNull(),
    createdAt: createdAt(),
    editedAt: timestamp("edited_at", { withTimezone: true }),
    deletedAt: timestamp("deleted_at", { withTimezone: true }),
    resolvedAt: timestamp("resolved_at", { withTimezone: true }),
    resolvedById: uuid("resolved_by_id"),
  },
  (table) => [
    unique().on(table.organisationId, table.issueId, table.id),
    foreignKey({
      columns: [table.organisationId, table.resolvedById],
      foreignColumns: [users.organisationId, users.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.issueId],
      foreignColumns: [issues.organisationId, issues.id],
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.organisationId, table.issueId, table.parentCommentId],
      foreignColumns: [table.organisationId, table.issueId, table.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.authorId],
      foreignColumns: [users.organisationId, users.id],
    }),
  ],
);

export const issueActivity = pgTable(
  "issue_activity",
  {
    id: id(),
    organisationId: organisationId(),
    issueId: uuid("issue_id").notNull(),
    actorId: uuid("actor_id"),
    source: text({ enum: ["rest", "mcp", "github"] })
      .notNull()
      .default("rest"),
    kind: text().notNull(),
    data: jsonb().$type<Record<string, unknown>>().notNull(),
    createdAt: createdAt(),
  },
  (table) => [
    index().on(table.organisationId, table.issueId, table.createdAt),
    foreignKey({
      columns: [table.organisationId, table.issueId],
      foreignColumns: [issues.organisationId, issues.id],
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.organisationId, table.actorId],
      foreignColumns: [users.organisationId, users.id],
    }),
  ],
);

export const uploads = pgTable(
  "uploads",
  {
    id: id(),
    organisationId: organisationId(),
    issueId: uuid("issue_id"),
    clientId: uuid("client_id"),
    uploadedById: uuid("uploaded_by_id").notNull(),
    kind: text({ enum: ["chat", "image", "file"] }).notNull(),
    state: text({ enum: ["pending", "ready", "deleted"] })
      .notNull()
      .default("pending"),
    fileName: text("file_name").notNull(),
    contentType: text("content_type").notNull(),
    sizeBytes: integer("size_bytes").notNull(),
    sha256: text().notNull(),
    expiresAt: timestamp("expires_at", { withTimezone: true }),
    purgedAt: timestamp("purged_at", { withTimezone: true }),
    createdAt: createdAt(),
  },
  (table) => [
    unique().on(table.organisationId, table.clientId),
    index().on(table.organisationId, table.issueId, table.kind),
    foreignKey({
      columns: [table.organisationId, table.issueId],
      foreignColumns: [issues.organisationId, issues.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.uploadedById],
      foreignColumns: [users.organisationId, users.id],
    }),
    check(
      "upload_ownership",
      sql`(
    (${table.kind} = 'chat' and ${table.issueId} is null and ${table.clientId} is not null and ${table.expiresAt} is not null)
    or
    (${table.kind} in ('image', 'file') and ${table.issueId} is not null and ${table.clientId} is null and ${table.expiresAt} is null)
  )`,
    ),
    check("upload_size", sql`${table.sizeBytes} between 1 and 25000000`),
  ],
);

export const issueAliases = pgTable(
  "issue_identifier_aliases",
  {
    organisationId: organisationId(),
    identifier: text().notNull(),
    issueId: uuid("issue_id").notNull(),
  },
  (table) => [
    primaryKey({ columns: [table.organisationId, table.identifier] }),
    foreignKey({
      columns: [table.organisationId, table.issueId],
      foreignColumns: [issues.organisationId, issues.id],
    }).onDelete("cascade"),
  ],
);

export const issueRelations = pgTable(
  "issue_relations",
  {
    organisationId: organisationId(),
    sourceId: uuid("source_id").notNull(),
    targetId: uuid("target_id").notNull(),
    kind: text({ enum: ["blocks", "relatedTo", "duplicateOf"] }).notNull(),
  },
  (table) => [
    primaryKey({ columns: [table.organisationId, table.kind, table.sourceId, table.targetId] }),
    foreignKey({
      columns: [table.organisationId, table.sourceId],
      foreignColumns: [issues.organisationId, issues.id],
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.organisationId, table.targetId],
      foreignColumns: [issues.organisationId, issues.id],
    }).onDelete("cascade"),
    index().on(table.organisationId, table.targetId),
    check("issue_relations_distinct", sql`${table.sourceId} <> ${table.targetId}`),
  ],
);

export const decisionStatuses = ["proposed", "accepted", "rejected", "superseded"] as const;
export const evidenceKinds = [
  "ircMessage",
  "teamsMessage",
  "subspaceComment",
  "documentOrUrl",
  "meeting",
  "other",
] as const;

export const decisions = pgTable(
  "project_decisions",
  {
    id: id(),
    organisationId: organisationId(),
    projectId: uuid("project_id").notNull(),
    recordedById: uuid("recorded_by_id").notNull(),
    title: text().notNull(),
    supersedesId: uuid("supersedes_id"),
    supersededById: uuid("superseded_by_id"),
    decidedOn: date("decided_on").notNull(),
    status: text({ enum: decisionStatuses }).notNull().default("proposed"),
    decision: text().notNull(),
    context: text().notNull().default(""),
    why: text().notNull().default(""),
    implications: text().notNull().default(""),
    alternatives: text().notNull().default(""),
    tags: jsonb().$type<readonly string[]>().notNull().default([]),
    madeByType: text("made_by_type", { enum: ["human", "agent"] }).notNull(),
    madeByUserId: uuid("made_by_user_id"),
    madeByAgentName: text("made_by_agent_name"),
    madeByAgentRank: text("made_by_agent_rank", { enum: ["rearAdmiral", "captain", "crew"] }),
    madeByAgentModel: text("made_by_agent_model"),
    evidenceKind: text("evidence_kind", { enum: evidenceKinds }),
    evidenceReference: text("evidence_reference"),
    evidenceExcerpt: text("evidence_excerpt"),
    acceptedAt: timestamp("accepted_at", { withTimezone: true }),
    acceptedByUserId: uuid("accepted_by_user_id"),
    rejectedAt: timestamp("rejected_at", { withTimezone: true }),
    rejectedByUserId: uuid("rejected_by_user_id"),
    version: integer().default(1).notNull(),
    createdAt: createdAt(),
    updatedAt: updatedAt(),
  },
  (table) => [
    unique().on(table.organisationId, table.id),
    index().on(table.organisationId, table.projectId, table.decidedOn),
    foreignKey({
      columns: [table.organisationId, table.supersedesId],
      foreignColumns: [table.organisationId, table.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.supersededById],
      foreignColumns: [table.organisationId, table.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.projectId],
      foreignColumns: [projects.organisationId, projects.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.madeByUserId],
      foreignColumns: [users.organisationId, users.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.recordedById],
      foreignColumns: [users.organisationId, users.id],
    }),
  ],
);

export const decisionIssues = pgTable(
  "project_decision_issues",
  {
    organisationId: organisationId(),
    decisionId: uuid("decision_id").notNull(),
    issueId: uuid("issue_id").notNull(),
  },
  (table) => [
    primaryKey({ columns: [table.organisationId, table.decisionId, table.issueId] }),
    index().on(table.organisationId, table.issueId),
    foreignKey({
      columns: [table.organisationId, table.decisionId],
      foreignColumns: [decisions.organisationId, decisions.id],
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.organisationId, table.issueId],
      foreignColumns: [issues.organisationId, issues.id],
    }),
  ],
);

export const projectRepositories = pgTable(
  "project_repositories",
  {
    organisationId: organisationId(),
    projectId: uuid("project_id").notNull(),
    repository: text().notNull(),
    createdAt: createdAt(),
  },
  (table) => [
    primaryKey({ columns: [table.organisationId, table.projectId, table.repository] }),
    index().on(table.repository),
    foreignKey({
      columns: [table.organisationId, table.projectId],
      foreignColumns: [projects.organisationId, projects.id],
    }).onDelete("cascade"),
  ],
);

export const pullRequests = pgTable(
  "github_pull_requests",
  {
    id: id(),
    organisationId: organisationId(),
    repository: text().notNull(),
    number: integer().notNull(),
    title: text().notNull(),
    authorLogin: text("author_login"),
    state: text({ enum: ["open", "closed", "merged"] }).notNull(),
    isDraft: boolean("is_draft").notNull(),
    headBranch: text("head_branch").notNull(),
    baseBranch: text("base_branch").notNull(),
    mergedAt: timestamp("merged_at", { withTimezone: true }),
    closedAt: timestamp("closed_at", { withTimezone: true }),
    githubUpdatedAt: timestamp("github_updated_at", { withTimezone: true }).notNull(),
    lastObservedAt: timestamp("last_observed_at", { withTimezone: true }).defaultNow().notNull(),
  },
  (table) => [
    unique().on(table.organisationId, table.id),
    unique().on(table.organisationId, table.repository, table.number),
  ],
);

export const pullRequestLinks = pgTable(
  "issue_pull_request_links",
  {
    organisationId: organisationId(),
    issueId: uuid("issue_id").notNull(),
    pullRequestId: uuid("pull_request_id").notNull(),
    kind: text({ enum: ["closes", "mentions"] }).notNull(),
    sources: jsonb().$type<readonly string[]>().notNull(),
    active: boolean().notNull().default(true),
    completion: text({
      enum: [
        "none",
        "completed",
        "skippedTracker",
        "skippedArchived",
        "skippedTerminal",
        "skippedNoState",
      ],
    })
      .notNull()
      .default("none"),
  },
  (table) => [
    primaryKey({ columns: [table.organisationId, table.issueId, table.pullRequestId] }),
    index().on(table.organisationId, table.pullRequestId),
    foreignKey({
      columns: [table.organisationId, table.issueId],
      foreignColumns: [issues.organisationId, issues.id],
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.organisationId, table.pullRequestId],
      foreignColumns: [pullRequests.organisationId, pullRequests.id],
    }).onDelete("cascade"),
  ],
);

export const todos = pgTable(
  "issue_todos",
  {
    id: id(),
    organisationId: organisationId(),
    issueId: uuid("issue_id").notNull(),
    text: text().notNull(),
    position: integer().notNull(),
    doneAt: timestamp("done_at", { withTimezone: true }),
    doneById: uuid("done_by_id"),
    createdById: uuid("created_by_id").notNull(),
    updatedById: uuid("updated_by_id"),
    version: integer().default(1).notNull(),
    createdAt: createdAt(),
    updatedAt: updatedAt(),
  },
  (table) => [
    unique().on(table.organisationId, table.issueId, table.position),
    foreignKey({
      columns: [table.organisationId, table.issueId],
      foreignColumns: [issues.organisationId, issues.id],
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.organisationId, table.createdById],
      foreignColumns: [users.organisationId, users.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.updatedById],
      foreignColumns: [users.organisationId, users.id],
    }),
    foreignKey({
      columns: [table.organisationId, table.doneById],
      foreignColumns: [users.organisationId, users.id],
    }),
  ],
);

export const githubDeploymentStatuses = pgTable(
  "github_deployment_statuses",
  {
    organisationId: organisationId(),
    repository: text().notNull(),
    deploymentId: bigint("deployment_id", { mode: "number" }).notNull(),
    statusId: bigint("status_id", { mode: "number" }).notNull(),
    sha: text().notNull(),
    environment: text().notNull(),
    state: text().notNull(),
    occurredAt: timestamp("occurred_at", { withTimezone: true }).notNull(),
  },
  (table) => [
    primaryKey({
      columns: [table.organisationId, table.repository, table.deploymentId, table.statusId],
    }),
    index().on(table.organisationId, table.repository, table.occurredAt),
  ],
);

export const githubPollCheckpoints = pgTable(
  "github_poll_checkpoints",
  {
    organisationId: organisationId(),
    repository: text().notNull(),
    observedThrough: timestamp("observed_through", { withTimezone: true }).notNull(),
  },
  (table) => [primaryKey({ columns: [table.organisationId, table.repository] })],
);

export const auditLogs = pgTable(
  "audit_logs",
  {
    id: id(),
    organisationId: organisationId(),
    occurredAt: timestamp("occurred_at", { withTimezone: true }).defaultNow().notNull(),
    action: text().notNull(),
    actorType: text({ enum: ["human", "agent", "integration"] }).notNull(),
    actorId: uuid("actor_id"),
    actorDisplayName: text("actor_display_name"),
    actorRank: text("actor_rank", { enum: ["rearAdmiral", "captain", "crew"] }),
    source: text({ enum: ["rest", "mcp", "github"] }).notNull(),
    entityType: text("entity_type").notNull(),
    entityId: text("entity_id"),
    projectId: uuid("project_id"),
    summary: text().notNull(),
    changes: jsonb().$type<Record<string, unknown>>().notNull(),
  },
  (table) => [index().on(table.organisationId, table.occurredAt, table.id)],
);

export const issueSubscriptions = pgTable(
  "issue_subscriptions",
  {
    organisationId: organisationId(),
    issueId: uuid("issue_id").notNull(),
    userId: uuid("user_id").notNull(),
    subscribed: boolean().notNull().default(true),
    explicitChoice: boolean("explicit_choice").notNull().default(false),
  },
  (table) => [
    primaryKey({ columns: [table.organisationId, table.issueId, table.userId] }),
    foreignKey({
      columns: [table.organisationId, table.issueId],
      foreignColumns: [issues.organisationId, issues.id],
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.organisationId, table.userId],
      foreignColumns: [users.organisationId, users.id],
    }).onDelete("cascade"),
  ],
);

export const issueNotifications = pgTable(
  "issue_notifications",
  {
    id: id(),
    organisationId: organisationId(),
    userId: uuid("user_id").notNull(),
    issueId: uuid("issue_id").notNull(),
    projectId: uuid("project_id").notNull(),
    commentId: uuid("comment_id"),
    actorUserId: uuid("actor_user_id"),
    changeId: uuid("change_id").notNull(),
    type: text({ enum: ["mentioned", "issueCommentAdded", "issueStateChanged"] }).notNull(),
    title: text().notNull(),
    body: text().notNull(),
    readAt: timestamp("read_at", { withTimezone: true }),
    createdAt: createdAt(),
  },
  (table) => [
    unique().on(table.organisationId, table.userId, table.changeId),
    index().on(table.organisationId, table.userId, table.createdAt, table.id),
    foreignKey({
      columns: [table.organisationId, table.userId],
      foreignColumns: [users.organisationId, users.id],
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.organisationId, table.issueId],
      foreignColumns: [issues.organisationId, issues.id],
    }).onDelete("cascade"),
  ],
);

export const decisionRevisions = pgTable(
  "project_decision_revisions",
  {
    id: id(),
    organisationId: organisationId(),
    decisionId: uuid("decision_id").notNull(),
    number: integer().notNull(),
    snapshot: jsonb().notNull(),
    savedAt: timestamp("saved_at", { withTimezone: true }).notNull().defaultNow(),
    savedBy: jsonb("saved_by").notNull(),
    changedFields: jsonb("changed_fields").$type<readonly string[]>().notNull(),
  },
  (table) => [
    unique().on(table.organisationId, table.decisionId, table.number),
    foreignKey({
      columns: [table.organisationId, table.decisionId],
      foreignColumns: [decisions.organisationId, decisions.id],
    }).onDelete("cascade"),
  ],
);

export const reviewKinds = ["spec", "requirements", "design", "general"] as const;
export const reviewReasoning = ["minimal", "low", "medium", "high", "xhigh", "max"] as const;
export const reviewVerdicts = ["approved", "changesRequested", "commented"] as const;

export const issueDocuments = pgTable(
  "issue_documents",
  {
    id: id(),
    organisationId: organisationId(),
    issueId: uuid("issue_id").notNull(),
    originalProjectId: uuid("original_project_id").notNull(),
    reference: text().notNull(),
    type: text({ enum: reviewKinds }).notNull(),
    subtype: text(),
    authorId: uuid("author_id").notNull(),
    currentRevision: integer("current_revision").notNull().default(1),
    archivedAt: timestamp("archived_at", { withTimezone: true }),
    createdAt: createdAt(),
    updatedAt: updatedAt(),
  },
  (t) => [
    unique().on(t.organisationId, t.id),
    unique().on(t.organisationId, t.reference),
    index().on(t.organisationId, t.issueId),
    foreignKey({
      columns: [t.organisationId, t.issueId],
      foreignColumns: [issues.organisationId, issues.id],
    }).onDelete("cascade"),
    foreignKey({
      columns: [t.organisationId, t.originalProjectId],
      foreignColumns: [projects.organisationId, projects.id],
    }),
    foreignKey({
      columns: [t.organisationId, t.authorId],
      foreignColumns: [users.organisationId, users.id],
    }),
  ],
);

export const documentRevisions = pgTable(
  "issue_document_revisions",
  {
    organisationId: organisationId(),
    documentId: uuid("document_id").notNull(),
    number: integer().notNull(),
    title: text().notNull(),
    content: text().notNull(),
    note: text(),
    authorId: uuid("author_id").notNull(),
    createdAt: createdAt(),
  },
  (t) => [
    primaryKey({ columns: [t.organisationId, t.documentId, t.number] }),
    foreignKey({
      columns: [t.organisationId, t.documentId],
      foreignColumns: [issueDocuments.organisationId, issueDocuments.id],
    }).onDelete("cascade"),
    foreignKey({
      columns: [t.organisationId, t.authorId],
      foreignColumns: [users.organisationId, users.id],
    }),
  ],
);

export const documentCounters = pgTable(
  "project_document_counters",
  {
    organisationId: organisationId(),
    projectId: uuid("project_id").notNull(),
    number: integer().notNull(),
  },
  (t) => [
    primaryKey({ columns: [t.organisationId, t.projectId] }),
    foreignKey({
      columns: [t.organisationId, t.projectId],
      foreignColumns: [projects.organisationId, projects.id],
    }).onDelete("cascade"),
  ],
);

export const projectReviewRequirements = pgTable(
  "project_review_requirements",
  {
    organisationId: organisationId(),
    projectId: uuid("project_id").notNull(),
    kinds: jsonb()
      .$type<
        Record<
          (typeof reviewKinds)[number],
          { model: string; reasoning: (typeof reviewReasoning)[number] | null }[]
        >
      >()
      .notNull(),
  },
  (t) => [
    primaryKey({ columns: [t.organisationId, t.projectId] }),
    foreignKey({
      columns: [t.organisationId, t.projectId],
      foreignColumns: [projects.organisationId, projects.id],
    }).onDelete("cascade"),
  ],
);

export const issueReviews = pgTable(
  "issue_reviews",
  {
    id: id(),
    organisationId: organisationId(),
    issueId: uuid("issue_id").notNull(),
    kind: text({ enum: reviewKinds }).notNull(),
    model: text().notNull(),
    reasoning: text({ enum: reviewReasoning }),
    verdict: text({ enum: reviewVerdicts }).notNull(),
    subject: text(),
    revision: text(),
    documentId: uuid("document_id"),
    documentRevision: integer("document_revision"),
    notes: text(),
    recordedById: uuid("recorded_by_id").notNull(),
    createdAt: createdAt(),
  },
  (t) => [
    index().on(t.organisationId, t.issueId, t.createdAt),
    foreignKey({
      columns: [t.organisationId, t.issueId],
      foreignColumns: [issues.organisationId, issues.id],
    }).onDelete("cascade"),
    foreignKey({
      columns: [t.organisationId, t.recordedById],
      foreignColumns: [users.organisationId, users.id],
    }),
    foreignKey({
      columns: [t.organisationId, t.documentId, t.documentRevision],
      foreignColumns: [
        documentRevisions.organisationId,
        documentRevisions.documentId,
        documentRevisions.number,
      ],
    }).onDelete("cascade"),
    check(
      "review_document_revision",
      sql`(${t.documentId} is null) = (${t.documentRevision} is null)`,
    ),
  ],
);

export const initiativeStakeholders = pgTable(
  "initiative_stakeholders",
  {
    id: id(),
    organisationId: organisationId(),
    initiativeId: uuid("initiative_id").notNull(),
    name: text().notNull(),
    role: text().notNull(),
    slackHandle: text("slack_handle"),
    teamsHandle: text("teams_handle"),
  },
  (t) => [
    index().on(t.organisationId, t.initiativeId),
    foreignKey({
      columns: [t.organisationId, t.initiativeId],
      foreignColumns: [initiatives.organisationId, initiatives.id],
    }).onDelete("cascade"),
    check("stakeholder_contact", sql`${t.slackHandle} is not null or ${t.teamsHandle} is not null`),
  ],
);

export const projectUpdates = pgTable(
  "project_updates",
  {
    id: id(),
    organisationId: organisationId(),
    projectId: uuid("project_id").notNull(),
    authorId: uuid("author_id").notNull(),
    body: text().notNull(),
    health: text({ enum: ["onTrack", "atRisk", "offTrack"] }).notNull(),
    version: integer().notNull().default(1),
    editedAt: timestamp("edited_at", { withTimezone: true }),
    createdAt: createdAt(),
    updatedAt: updatedAt(),
  },
  (t) => [
    index().on(t.organisationId, t.projectId, t.createdAt, t.id),
    foreignKey({
      columns: [t.organisationId, t.projectId],
      foreignColumns: [projects.organisationId, projects.id],
    }).onDelete("cascade"),
    foreignKey({
      columns: [t.organisationId, t.authorId],
      foreignColumns: [users.organisationId, users.id],
    }),
  ],
);

export const issueViews = pgTable(
  "issue_views",
  {
    id: id(),
    organisationId: organisationId(),
    ownerId: uuid("owner_id").notNull(),
    name: text().notNull(),
    visibility: text({ enum: ["personal", "shared"] }).notNull(),
    definition: jsonb().notNull(),
    version: integer().notNull().default(1),
    createdAt: createdAt(),
    updatedAt: updatedAt(),
  },
  (t) => [
    uniqueIndex("issue_views_owner_name").on(t.organisationId, t.ownerId, sql`lower(${t.name})`),
    index().on(t.organisationId, t.visibility),
    foreignKey({
      columns: [t.organisationId, t.ownerId],
      foreignColumns: [users.organisationId, users.id],
    }).onDelete("cascade"),
  ],
);

export const cycleResults = pgTable(
  "issue_cycle_results",
  {
    organisationId: organisationId(),
    projectId: uuid("project_id").notNull(),
    cycleId: uuid("cycle_id").notNull(),
    issueId: uuid("issue_id").notNull(),
    stateType: text("state_type", { enum: stateTypes }).notNull(),
    stateName: text("state_name").notNull(),
    points: doublePrecision(),
    outcome: text({ enum: ["completed", "canceled", "carriedOver", "notCompleted"] }).notNull(),
    joinedAfterStart: boolean("joined_after_start").notNull().default(false),
  },
  (t) => [
    primaryKey({ columns: [t.organisationId, t.cycleId, t.issueId] }),
    foreignKey({
      columns: [t.organisationId, t.projectId, t.cycleId],
      foreignColumns: [cycles.organisationId, cycles.projectId, cycles.id],
    }).onDelete("cascade"),
    foreignKey({
      columns: [t.organisationId, t.issueId],
      foreignColumns: [issues.organisationId, issues.id],
    }).onDelete("cascade"),
  ],
);
