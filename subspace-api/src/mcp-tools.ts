import {
  CreateFleet,
  UpdateFleet,
  FleetRef,
  CreateShip,
  RenameShip,
  createFleet,
  updateFleet,
  getFleet,
  listFleets,
  createShip,
  listShips,
  renameShip,
} from "./fleets";
import { Effect, Schema } from "effect";
import { changeComment, getComment, locateComment } from "./comment-mutations";
import { AuditQuery, listAudit } from "./audit";
import { Database } from "./db";
import {
  changeDecision,
  CountQuery as DecisionCountQuery,
  countDecisions,
  createDecision,
  DecisionEdit,
  getDecision,
  listDecisions,
  listRevisions,
  NewDecision,
  Query as DecisionQuery,
  StatusChange,
} from "./decisions";
import {
  changeDocument,
  createDocument,
  CreateDocument,
  DocumentQuery,
  EditDocument,
  getDocument,
  listDocumentRevisions,
  listDocuments,
} from "./documents";
import {
  documentReviewStatus,
  getRequirements,
  issueReviewStatus,
  IssueReviewStatus,
  listReviews,
  ListReviewQuery,
  recordReview,
  RecordReview,
  RequirementsInput,
  setRequirements,
} from "./reviews";
import { type Member, publicProfile } from "./identity";
import { Version } from "./initiative-contract";
import { memberInitiatives } from "./initiatives";
import {
  CommentInput,
  EditComment,
  EditIssue,
  GroupQuery,
  IssueQuery,
  NewIssue,
  RemoveComment,
  ResolveComment,
} from "./issue-contract";
import {
  createComment,
  createIssue,
  editIssue,
  getIssue,
  groupIssues,
  issueComments,
  issueTimeline,
  listIssues,
  listRelations,
  memberQueue,
  setIssueArchived,
} from "./issues";
import { handleDb } from "./middleware";
import {
  chooseSubscription,
  listNotifications,
  listSubscribers,
  markAllNotifications,
  markNotification,
  NotificationQuery,
} from "./notifications";
import { MoveQuery, MoveRequest, moveIssue, previewMove } from "./moves";
import { endpointErrors, problem } from "./problems";
import {
  getProject,
  createLabel,
  createMilestone,
  LabelInput,
  listCycles,
  listLabels,
  listMilestones,
  MilestoneInput,
  listProjects,
  listUsers,
  ProjectQuery,
  projectStates,
  readProject,
} from "./projects";
import { Add, Edit, Reorder, add, edit, list, reorder } from "./todos";
import { Line, PageQuery, Ref, Uuid } from "./validation";
import { GithubIntegration, listPullRequests, repoList, SetRepos, setRepos } from "./github";
import { changeView, EditView, getView, listViews, NewView } from "./views";
import {
  addCycleIssues,
  AddCycleIssues,
  cycleDetail,
  listCycleResults,
  listCycleTrackers,
  reconcileCycles,
  SetCycleSettings,
} from "./cycles";
import {
  changeEstimateScale,
  EstimateInput,
  previewEstimates,
  replaceWorkflow,
  ReplaceWorkflow,
} from "./workflow";
import { EstimateScale } from "./estimates";
import {
  changeLabel,
  changeMilestone,
  changeProjectUpdate,
  listProjectUpdates,
  MilestoneChange,
  ProjectUpdateEdit,
  ProjectUpdateInput,
  ProjectUpdateQuery,
} from "./planning";

type ToolProblem = (typeof endpointErrors)[number]["Type"];

function tool<S extends Schema.Decoder<unknown>, A, E>(
  description: string,
  readOnly: boolean,
  schema: S,
  run: (member: Member, args: S["Type"]) => Effect.Effect<A, E, Database | GithubIntegration>,
) {
  const document = Schema.toJsonSchemaDocument(schema, { onExcessProperty: "error" });
  return {
    description,
    inputSchema: { ...document.schema, type: "object", $defs: document.definitions },
    annotations: { readOnlyHint: readOnly, openWorldHint: false },
    call: (
      member: Member,
      input: unknown,
    ): Effect.Effect<unknown, ToolProblem, Database | GithubIntegration> =>
      Effect.flatMap(
        Schema.decodeUnknownEffect(schema, { onExcessProperty: "error" })(input).pipe(
          Effect.mapError(() => problem(400, "Check the tool argument fields and types.")),
        ),
        (args) => handleDb(run(member, args)),
      ),
  };
}

const issue = { issue: Ref };
const project = { project: Ref };
const Issue = Schema.Struct(issue);
const IssuePage = Schema.Struct({ ...issue, ...PageQuery });
const Pagination = Schema.Struct(PageQuery);
const NewIssueInProject = Schema.Struct({
  ...NewIssue.fields,
  projectId: Schema.optional(Uuid),
  project: Ref,
});

const { projectId: _projectId, ...decisionFields } = NewDecision.fields;
const decision = { decisionId: Uuid };
const document = { document: Ref };
const comment = { ...issue, commentId: Uuid };
const view = { viewId: Uuid };
const cycle = { cycleId: Uuid };
const milestone = { ...project, milestoneId: Uuid };
const update = { ...project, updateId: Uuid };
export const mcpTools = {
  subspace_list_issue_views: tool(
    "List personal and shared issue views.",
    true,
    Pagination,
    listViews,
  ),
  subspace_get_issue_view: tool(
    "Get a saved issue view.",
    true,
    Schema.Struct(view),
    (member, { viewId }) => getView(member, viewId),
  ),
  subspace_create_issue_view: tool(
    "Create a personal or shared issue view.",
    false,
    NewView,
    (member, input) => changeView(member, { create: input }),
  ),
  subspace_update_issue_view: tool(
    "Update a saved issue view using its current version.",
    false,
    Schema.Struct({ ...view, ...EditView.fields }),
    (member, { viewId, ...edit }) => changeView(member, { id: viewId, edit }),
  ),
  subspace_delete_issue_view: tool(
    "Delete a saved issue view.",
    false,
    Schema.Struct({ ...view, version: Version }),
    (member, { viewId, version }) => changeView(member, { id: viewId, remove: version }),
  ),
  subspace_list_milestones: tool(
    "List a project's milestones.",
    true,
    Schema.Struct({ ...project, ...PageQuery, includeContent: Schema.optional(Schema.Boolean) }),
    (member, { project, ...query }) => listMilestones(member, project, query),
  ),
  subspace_create_milestone: tool(
    "Create a project milestone.",
    false,
    Schema.Struct({ ...project, ...MilestoneInput.fields }),
    (member, { project, ...input }) => createMilestone(member, project, input),
  ),
  subspace_update_milestone: tool(
    "Update a project milestone.",
    false,
    Schema.Struct({ ...milestone, ...MilestoneChange.fields }),
    (member, { project, milestoneId, ...input }) =>
      changeMilestone(member, project, milestoneId, input),
  ),
  subspace_delete_milestone: tool(
    "Delete a project milestone and remove issue assignments.",
    false,
    Schema.Struct({ ...milestone, version: Version }),
    (member, { project, milestoneId, version }) =>
      changeMilestone(member, project, milestoneId, { version, remove: true }),
  ),
  subspace_list_project_updates: tool(
    "Read project progress updates.",
    true,
    Schema.Struct({ ...project, ...ProjectUpdateQuery.fields }),
    (member, { project, ...query }) => listProjectUpdates(member, project, query),
  ),
  subspace_create_project_update: tool(
    "Post a project progress update.",
    false,
    Schema.Struct({ ...project, ...ProjectUpdateInput.fields }),
    (member, { project, ...create }) => changeProjectUpdate(member, project, { create }),
  ),
  subspace_update_project_update: tool(
    "Edit a project progress update.",
    false,
    Schema.Struct({ ...update, ...ProjectUpdateEdit.fields }),
    (member, { project, updateId, ...edit }) =>
      changeProjectUpdate(member, project, { id: updateId, edit }),
  ),
  subspace_delete_project_update: tool(
    "Delete a project progress update.",
    false,
    Schema.Struct({ ...update, version: Version }),
    (member, { project, updateId, version }) =>
      changeProjectUpdate(member, project, { id: updateId, remove: version }),
  ),
  subspace_list_cycles: tool(
    "List issue cycles.",
    true,
    Schema.Struct({ ...PageQuery, project: Schema.optional(Ref) }),
    listCycles,
  ),
  subspace_get_cycle: tool(
    "Get an issue cycle.",
    true,
    Schema.Struct(cycle),
    (member, { cycleId }) => cycleDetail(member, cycleId),
  ),
  subspace_list_cycle_results: tool(
    "Read the immutable issue outcomes for a finished cycle.",
    true,
    Schema.Struct({ ...cycle, ...PageQuery }),
    (member, { cycleId, ...query }) => listCycleResults(member, cycleId, query),
  ),
  subspace_list_cycle_trackers: tool(
    "List trackers with children in a cycle.",
    true,
    Schema.Struct({ ...cycle, ...PageQuery }),
    (member, { cycleId, ...query }) => listCycleTrackers(member, cycleId, query),
  ),
  subspace_update_cycle_settings: tool(
    "Change a project's issue cycle schedule.",
    false,
    Schema.Struct({ ...project, ...SetCycleSettings.fields }),
    (member, { project, ...input }) => reconcileCycles(member, project, input),
  ),
  subspace_reconcile_cycles: tool(
    "Advance a project's cycle schedule.",
    false,
    Schema.Struct(project),
    (member, { project }) => reconcileCycles(member, project),
  ),
  subspace_add_issues_to_cycle: tool(
    "Assign issues to a current or upcoming cycle.",
    false,
    Schema.Struct({ ...cycle, ...AddCycleIssues.fields }),
    (member, { cycleId, issues }) => addCycleIssues(member, cycleId, issues),
  ),
  subspace_replace_issue_statuses: tool(
    "Replace project workflow state settings.",
    false,
    Schema.Struct({ ...project, ...ReplaceWorkflow.fields }),
    (member, { project, ...input }) => replaceWorkflow(member, project, input),
  ),
  subspace_preview_estimate_scale: tool(
    "Preview changes from selecting an estimate scale.",
    true,
    Schema.Struct({ ...project, scale: EstimateScale }),
    (member, { project, scale }) => previewEstimates(member, project, scale),
  ),
  subspace_update_estimate_scale: tool(
    "Set the project estimate scale.",
    false,
    Schema.Struct({ ...project, ...EstimateInput.fields }),
    (member, { project, ...input }) => changeEstimateScale(member, project, input),
  ),
  subspace_list_project_repositories: tool(
    "List a project's mapped repositories.",
    true,
    Schema.Struct(project),
    (member, { project }) => repoList(member, project),
  ),
  subspace_set_project_repositories: tool(
    "Set a project's mapped repositories.",
    false,
    Schema.Struct({ ...project, ...SetRepos.fields }),
    (member, { project, ...input }) => setRepos(member, project, input),
  ),
  subspace_create_label: tool("Create an issue label.", false, LabelInput, createLabel),
  subspace_update_label: tool(
    "Update an issue label.",
    false,
    Schema.Struct({ labelId: Uuid, ...LabelInput.fields }),
    (member, { labelId, ...input }) => changeLabel(member, labelId, input),
  ),

  subspace_get_comment: tool(
    "Read a comment on an issue.",
    true,
    Schema.Struct(comment),
    (member, { issue, commentId }) => getComment(member, issue, commentId),
  ),
  subspace_get_comment_location: tool(
    "Locate a comment and its reply window.",
    true,
    Schema.Struct({ commentId: Uuid }),
    (member, { commentId }) => locateComment(member, commentId),
  ),
  subspace_edit_comment: tool(
    "Edit a comment that you authored.",
    false,
    Schema.Struct({ ...comment, ...EditComment.fields }),
    (member, { issue, commentId, ...input }) =>
      changeComment(member, issue, commentId, { ...input, action: "edit" }),
  ),
  subspace_delete_comment: tool(
    "Delete a comment while retaining its thread.",
    false,
    Schema.Struct({ ...comment, ...RemoveComment.fields }),
    (member, { issue, commentId, ...input }) =>
      changeComment(member, issue, commentId, { ...input, action: "remove" }),
  ),
  subspace_set_comment_resolution: tool(
    "Resolve or reopen a root comment.",
    false,
    Schema.Struct({ ...issue, rootId: Uuid, ...ResolveComment.fields }),
    (member, { issue, rootId, ...input }) =>
      changeComment(member, issue, rootId, { ...input, action: "resolve" }),
  ),
  subspace_list_documents: tool(
    "List documents attached to an issue.",
    true,
    Schema.Struct({ ...issue, ...DocumentQuery.fields }),
    (member, { issue, ...query }) => listDocuments(member, issue, query),
  ),
  subspace_get_document: tool(
    "Get a document by reference or UUID.",
    true,
    Schema.Struct(document),
    (member, { document }) => getDocument(member, document),
  ),
  subspace_create_document: tool(
    "Create a versioned document on an issue.",
    false,
    Schema.Struct({ ...issue, ...CreateDocument.fields }),
    (member, { issue, ...input }) => createDocument(member, issue, input),
  ),
  subspace_update_document: tool(
    "Save a new revision of a document.",
    false,
    Schema.Struct({ ...document, ...EditDocument.fields }),
    (member, { document, ...input }) => changeDocument(member, document, input),
  ),
  subspace_archive_document: tool(
    "Archive a document.",
    false,
    Schema.Struct(document),
    (member, { document }) => changeDocument(member, document, true),
  ),
  subspace_unarchive_document: tool(
    "Restore a document.",
    false,
    Schema.Struct(document),
    (member, { document }) => changeDocument(member, document, false),
  ),
  subspace_list_document_revisions: tool(
    "List revisions of a document.",
    true,
    Schema.Struct({ ...document, ...PageQuery }),
    (member, { document, ...query }) => listDocumentRevisions(member, document, query),
  ),
  subspace_get_review_requirements: tool(
    "Get project review requirements.",
    true,
    Schema.Struct(project),
    (member, { project }) => getRequirements(member, project),
  ),
  subspace_set_review_requirements: tool(
    "Replace review requirements using the current project version.",
    false,
    Schema.Struct({ ...project, ...RequirementsInput.fields }),
    (member, { project, ...input }) => setRequirements(member, project, input),
  ),
  subspace_record_review: tool(
    "Record review evidence for an issue or document revision.",
    false,
    Schema.Struct({ ...issue, ...RecordReview.fields }),
    (member, { issue, ...input }) => recordReview(member, issue, input),
  ),
  subspace_list_reviews: tool(
    "List recorded reviews.",
    true,
    Schema.Struct({ ...issue, ...ListReviewQuery.fields }),
    (member, { issue, ...query }) => listReviews(member, issue, query),
  ),
  subspace_get_review_status: tool(
    "Check required reviews against a document or external revision.",
    true,
    Schema.Union([
      Schema.Struct({ ...issue, ...IssueReviewStatus.fields }),
      Schema.Struct({ ...document, documentRevision: Schema.optional(Version) }),
    ]),
    (member, input): Effect.Effect<unknown, unknown, Database> =>
      "document" in input
        ? documentReviewStatus(member, input.document, input.documentRevision)
        : issueReviewStatus(member, input.issue, input),
  ),
  subspace_list_fleets: tool("List fleets.", true, Pagination, listFleets),
  subspace_get_fleet: tool(
    "Read a fleet by ID or slug.",
    true,
    Schema.Struct({ reference: FleetRef }),
    (member, { reference }) => getFleet(member, reference),
  ),
  subspace_create_fleet: tool(
    "Create a fleet. Requires a human administrator.",
    false,
    Schema.Struct({ fleet: CreateFleet }),
    (member, { fleet }) => createFleet(member, fleet),
  ),
  subspace_update_fleet: tool(
    "Update a fleet. Requires a human administrator.",
    false,
    Schema.Struct({ reference: FleetRef, changes: UpdateFleet }),
    (member, { reference, changes }) => updateFleet(member, reference, changes),
  ),
  subspace_list_ships: tool(
    "List ships in a fleet.",
    true,
    Schema.Struct({ fleetId: Uuid }),
    (member, { fleetId }) => listShips(member, fleetId),
  ),
  subspace_create_ship: tool(
    "Create a ship in a fleet.",
    false,
    Schema.Struct({ fleetId: Uuid, ...CreateShip.fields }),
    (member, { fleetId, name, channel }) => createShip(member, fleetId, name, channel),
  ),
  subspace_rename_ship: tool(
    "Rename a ship.",
    false,
    Schema.Struct({ shipId: Uuid, ...RenameShip.fields }),
    (member, { shipId, ...changes }) => renameShip(member, shipId, changes),
  ),
  subspace_list_decisions: tool(
    "List project decisions with optional filters.",
    true,
    Schema.Struct({ filters: Schema.optional(DecisionQuery) }),
    (member, { filters }) => listDecisions(member, filters ?? {}),
  ),
  subspace_count_decisions: tool(
    "Count project decisions by status with optional filters.",
    true,
    Schema.Struct({ filters: Schema.optional(DecisionCountQuery) }),
    (member, { filters }) => countDecisions(member, filters ?? {}),
  ),
  subspace_get_decision: tool(
    "Get the current project decision.",
    true,
    Schema.Struct(decision),
    (member, { decisionId }) =>
      Effect.flatMap(Database, (db) => getDecision(db, member, decisionId)),
  ),
  subspace_log_decision: tool(
    "Record a project decision.",
    false,
    Schema.Struct({ ...project, ...decisionFields }),
    (member, { project, ...input }) =>
      Effect.gen(function* () {
        const found = yield* readProject(yield* Database, member, project);
        return yield* createDecision(member, { ...input, projectId: found.project.id });
      }),
  ),
  subspace_update_decision: tool(
    "Edit a proposed decision.",
    false,
    Schema.Struct({ ...decision, changes: DecisionEdit }),
    (member, { decisionId, changes }) => changeDecision(member, decisionId, changes),
  ),
  subspace_set_decision_status: tool(
    "Accept or reject a proposed decision.",
    false,
    Schema.Struct({ ...decision, ...StatusChange.fields }),
    (member, { decisionId, ...input }) => changeDecision(member, decisionId, input),
  ),
  subspace_list_decision_revisions: tool(
    "List immutable decision revisions.",
    true,
    Schema.Struct({ ...decision, ...PageQuery }),
    (member, { decisionId, ...query }) => listRevisions(member, decisionId, query),
  ),
  subspace_list_notifications: tool(
    "List your issue notifications.",
    true,
    NotificationQuery,
    listNotifications,
  ),
  subspace_mark_notification_read: tool(
    "Mark a notification read.",
    false,
    Schema.Struct({ notificationId: Uuid }),
    (member, args) => markNotification(member, args.notificationId, true),
  ),
  subspace_mark_notification_unread: tool(
    "Mark a notification unread.",
    false,
    Schema.Struct({ notificationId: Uuid }),
    (member, args) => markNotification(member, args.notificationId, false),
  ),
  subspace_mark_all_notifications_read: tool(
    "Mark your entire inbox read.",
    false,
    Schema.Struct({}),
    markAllNotifications,
  ),
  subspace_list_issue_subscribers: tool(
    "List people following an issue.",
    true,
    IssuePage,
    (member, { issue, ...query }) => listSubscribers(member, issue, query),
  ),
  subspace_set_issue_subscription: tool(
    "Choose whether to follow an issue.",
    false,
    Schema.Struct({ ...issue, subscribed: Schema.Boolean }),
    (member, args) => chooseSubscription(member, args.issue, args.subscribed),
  ),
  subspace_list_audit: tool("List audit events visible to you.", true, AuditQuery, listAudit),
  subspace_whoami: tool("Read your authenticated profile.", true, Schema.Struct({}), (member) =>
    Effect.succeed(publicProfile(member)),
  ),
  subspace_list_initiatives: tool(
    "List initiatives you belong to.",
    true,
    Schema.Struct({}),
    (member) => Effect.map(memberInitiatives(member), ({ items }) => ({ items, nextCursor: null })),
  ),
  subspace_list_users: tool("List active people and agents.", true, Pagination, listUsers),
  subspace_list_projects: tool(
    "List projects with cursor paging.",
    true,
    ProjectQuery,
    listProjects,
  ),
  subspace_get_project: tool(
    "Get a project by key or UUID.",
    true,
    Schema.Struct(project),
    (member, args) => getProject(member, args.project),
  ),
  subspace_list_issue_statuses: tool(
    "List a project's workflow states.",
    true,
    Schema.Struct({ ...project, ...PageQuery }),
    (member, { project, ...query }) => projectStates(member, project, query),
  ),
  subspace_list_labels: tool(
    "List issue labels.",
    true,
    Schema.Struct({ ...PageQuery, includeArchived: Schema.optional(Schema.Boolean) }),
    listLabels,
  ),
  subspace_list_issues: tool(
    "List issues with optional filters and cursor paging.",
    true,
    Schema.Struct({ filters: Schema.optional(IssueQuery) }),
    (member, args) => listIssues(member, args.filters ?? {}),
  ),
  subspace_list_my_queue: tool(
    "List open issues assigned to you in priority order.",
    true,
    Pagination,
    memberQueue,
  ),
  subspace_get_issue: tool(
    "Read issue detail and the current version.",
    true,
    Issue,
    (member, args) => getIssue(member, args.issue),
  ),
  subspace_create_issue: tool(
    "Create an issue in a project identified by its key or UUID. A fleet agent that sets a non-null assigneeId or sends readyForAgents receives 403 when the project belongs to another fleet.",
    false,
    NewIssueInProject,
    (member, { project, projectId, ...payload }) =>
      Effect.gen(function* () {
        const found = yield* readProject(yield* Database, member, project);
        if (projectId && found.project.id !== projectId)
          return yield* Effect.fail(problem(400, "The project references do not match."));
        return yield* createIssue(member, { ...payload, projectId: found.project.id });
      }),
  ),
  subspace_update_issue: tool(
    "Patch an issue using its current version. A fleet agent that sets a non-null assigneeId or sends readyForAgents receives 403 when the issue's project belongs to another fleet.",
    false,
    Schema.Struct({ ...issue, changes: EditIssue }),
    (member, args) => editIssue(member, args.issue, args.changes),
  ),
  subspace_group_issues: tool(
    "Group a project's issues by workflow state.",
    true,
    Schema.Struct({ filters: GroupQuery }),
    (member, args) => groupIssues(member, args.filters),
  ),
  subspace_list_issue_relations: tool(
    "List an issue's relationships.",
    true,
    IssuePage,
    (member, { issue, ...query }) => listRelations(member, issue, query),
  ),
  subspace_list_issue_activity: tool(
    "List an issue's activity.",
    true,
    IssuePage,
    (member, { issue, ...query }) => issueTimeline(member, issue, query),
  ),
  subspace_list_issue_pull_requests: tool(
    "List pull requests linked to an issue.",
    true,
    Schema.Struct({ ...issue, ...PageQuery, includeInactive: Schema.optional(Schema.Boolean) }),
    (member, { issue, ...query }) => listPullRequests(member, issue, query),
  ),
  subspace_preview_issue_move: tool(
    "Preview the impact of moving an issue.",
    true,
    Schema.Struct({ ...issue, ...MoveQuery.fields }),
    (member, { issue, ...input }) => previewMove(member, issue, input),
  ),
  subspace_move_issue: tool(
    "Move an issue using a current preview token. A fleet agent receives 403 when the source or destination project belongs to another fleet.",
    false,
    Schema.Struct({ ...issue, ...MoveRequest.fields }),
    (member, { issue, ...input }) => moveIssue(member, issue, input),
  ),
  subspace_archive_issue: tool(
    "Archive an issue.",
    false,
    Schema.Struct({ ...issue, version: Schema.optional(Version) }),
    (member, args) => setIssueArchived(member, args.issue, true, args.version),
  ),
  subspace_unarchive_issue: tool(
    "Restore an archived issue.",
    false,
    Schema.Struct({ ...issue, version: Schema.optional(Version) }),
    (member, args) => setIssueArchived(member, args.issue, false, args.version),
  ),
  subspace_list_comments: tool(
    "List an issue's root comments.",
    true,
    IssuePage,
    (member, { issue, ...query }) => issueComments(member, issue, query),
  ),
  subspace_list_comment_replies: tool(
    "List replies to a root comment.",
    true,
    Schema.Struct({ ...issue, ...PageQuery, rootId: Uuid }),
    (member, { issue, rootId, ...query }) => issueComments(member, issue, query, rootId),
  ),
  subspace_create_comment: tool(
    "Add a root comment or reply.",
    false,
    Schema.Struct({ ...issue, ...CommentInput.fields }),
    (member, { issue, ...input }) => createComment(member, issue, input),
  ),
  subspace_list_todos: tool("List the issue checklist.", true, Issue, (member, args) =>
    list(member, args.issue),
  ),
  subspace_add_todos: tool(
    "Add checklist items.",
    false,
    Schema.Struct({ ...issue, ...Add.fields }),
    (member, { issue, ...args }) => add(member, issue, args),
  ),
  subspace_update_todo: tool(
    "Edit a checklist item using its version.",
    false,
    Schema.Struct({ ...issue, todoId: Uuid, ...Edit.fields }),
    (member, { issue, todoId, ...args }) => edit(member, issue, todoId, args),
  ),
  subspace_delete_todo: tool(
    "Delete a checklist item using its version.",
    false,
    Schema.Struct({ ...issue, todoId: Uuid, version: Version }),
    (member, { issue, todoId, version }) => edit(member, issue, todoId, { version }, true),
  ),
  subspace_reorder_todos: tool(
    "Reorder all checklist items.",
    false,
    Schema.Struct({ ...issue, ...Reorder.fields }),
    (member, { issue, todoIds }) => reorder(member, issue, todoIds),
  ),
} as const;

export const ToolName = Line(100);
