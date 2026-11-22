import { Version } from "./initiative-contract";
import { Effect, Schema } from "effect";
import { HttpApiEndpoint, HttpApiGroup, HttpApiSchema, OpenApi } from "effect/http-api";
import {
  ActivityPage,
  ActivityQuery,
  Comment,
  CommentInput,
  EditIssue,
  GroupQuery,
  IssueDetail,
  IssueFields,
  IssueGroups,
  IssueQuery,
  NewIssue,
  Relation,
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
  memberQueue,
  listRelations,
  setIssueArchived,
} from "./issues";
import { MovePreview, MoveQuery, MoveRequest, MoveResult, moveIssue, previewMove } from "./moves";
import { endpointErrors } from "./problems";
import {
  assignProject,
  ProjectAssignment,
  createLabel,
  createMilestone,
  createProject,
  Cycle,
  getProject,
  editProject,
  ProjectUpdate,
  Label,
  LabelInput,
  listCycles,
  listLabels,
  listMilestones,
  listProjects,
  listUsers,
  Milestone,
  MilestoneInput,
  Project,
  ProjectInput,
  ProjectQuery,
  projectStates,
  ProjectState,
  UserChoice,
} from "./projects";
import { pageOf, PageQuery, Ref, Uuid } from "./validation";
import { BearerAuth, handleDb, ValidationErrors } from "./middleware";
import { CurrentMember } from "./identity";
import { versionedApi } from "./middleware";

const params = { reference: Ref };
const fleetOwnership =
  "An agent registered to a fleet that sets a non-null assigneeId, or sends readyForAgents, receives 403 when the issue's project belongs to another fleet. A project belongs to its initiative's fleet, or else to the fleet of that initiative's nearest ancestor with one.";
const optionalVersion = Schema.Struct({ version: Schema.optional(Version) });

export const projectGroup = HttpApiGroup.make("projects")
  .add(
    HttpApiEndpoint.get("projects", "/projects", {
      query: ProjectQuery,
      success: pageOf(Project),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("project", "/projects/:reference", {
      params,
      success: Project,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("createProject", "/projects", {
      payload: ProjectInput,
      success: Project.pipe(HttpApiSchema.status(201)),
      error: endpointErrors,
    }),
    HttpApiEndpoint.put("updateProject", "/projects/:reference", {
      params,
      payload: ProjectUpdate,
      success: Project,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("archiveProject", "/projects/:reference/archive", {
      params,
      payload: optionalVersion,
      success: Project,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("unarchiveProject", "/projects/:reference/unarchive", {
      params,
      payload: optionalVersion,
      success: Project,
      error: endpointErrors,
    }),
    HttpApiEndpoint.put("assignProject", "/projects/:reference/initiative", {
      params,
      payload: ProjectAssignment,
      success: Project,
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("workflowStates", "/projects/:reference/workflow-states", {
      params,
      query: PageQuery,
      success: pageOf(ProjectState),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("milestones", "/projects/:reference/milestones", {
      params,
      query: {
        ...PageQuery,
        milestoneId: Schema.optional(Uuid),
        includeContent: Schema.optional(Schema.Boolean),
      },
      success: pageOf(Milestone),
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("createMilestone", "/projects/:reference/milestones", {
      params,
      payload: MilestoneInput,
      success: Milestone.pipe(HttpApiSchema.status(201)),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("cycles", "/issue-cycles", {
      query: { ...PageQuery, project: Schema.optional(Ref) },
      success: pageOf(Cycle),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("users", "/users", {
      query: PageQuery,
      success: pageOf(UserChoice),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("labels", "/labels", {
      query: { ...PageQuery, includeArchived: Schema.optional(Schema.Boolean) },
      success: pageOf(Label),
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("createLabel", "/labels", {
      payload: LabelInput,
      success: Label.pipe(HttpApiSchema.status(201)),
      error: endpointErrors,
    }),
  )
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const issueGroup = HttpApiGroup.make("issues")
  .add(
    HttpApiEndpoint.get("queue", "/me/queue", {
      query: PageQuery,
      success: pageOf(IssueFields),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("issues", "/issues", {
      query: IssueQuery,
      success: pageOf(IssueFields),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("groups", "/issues/groups", {
      query: GroupQuery,
      success: IssueGroups,
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("issue", "/issues/:reference", {
      params,
      success: IssueDetail,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("createIssue", "/issues", {
      payload: NewIssue,
      success: IssueDetail.pipe(HttpApiSchema.status(201)),
      error: endpointErrors,
    }).annotate(OpenApi.Description, fleetOwnership),
    HttpApiEndpoint.patch("editIssue", "/issues/:reference", {
      params,
      payload: EditIssue,
      success: IssueDetail,
      error: endpointErrors,
    }).annotate(OpenApi.Description, fleetOwnership),
    HttpApiEndpoint.post("archiveIssue", "/issues/:reference/archive", {
      params,
      payload: optionalVersion,
      success: IssueDetail,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("unarchiveIssue", "/issues/:reference/unarchive", {
      params,
      payload: optionalVersion,
      success: IssueDetail,
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("comments", "/issues/:reference/comments", {
      params,
      query: PageQuery,
      success: pageOf(Comment),
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("createComment", "/issues/:reference/comments", {
      params,
      payload: CommentInput,
      success: Comment.pipe(HttpApiSchema.status(201)),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("replies", "/issues/:reference/comments/:root/replies", {
      params: { ...params, root: Uuid },
      query: PageQuery,
      success: pageOf(Comment),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("relations", "/issues/:reference/relations", {
      params,
      query: PageQuery,
      success: pageOf(Relation),
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("movePreview", "/issues/:reference/move-preview", {
      params,
      payload: MoveQuery,
      success: MovePreview,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("moveIssue", "/issues/:reference/move", {
      params,
      payload: MoveRequest,
      success: MoveResult,
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      "An agent registered to a fleet receives 403 when the source or destination project belongs to another fleet. The preview does not apply this check.",
    ),
    HttpApiEndpoint.get("activity", "/issues/:reference/activity", {
      params,
      query: ActivityQuery,
      success: ActivityPage,
      error: endpointErrors,
    }),
  )
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const projectsHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof projectGroup>,
) =>
  handlers
    .handle("projects", ({ query }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => listProjects(member, query))),
    )
    .handle("project", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => getProject(member, params.reference))),
    )
    .handle("createProject", ({ payload }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => createProject(member, payload))),
    )
    .handle("updateProject", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          editProject(member, params.reference, { fields: payload }),
        ),
      ),
    )
    .handle("archiveProject", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          editProject(member, params.reference, { archived: true, version: payload.version }),
        ),
      ),
    )
    .handle("unarchiveProject", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          editProject(member, params.reference, { archived: false, version: payload.version }),
        ),
      ),
    )
    .handle("assignProject", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => assignProject(member, params.reference, payload)),
      ),
    )
    .handle("workflowStates", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => projectStates(member, params.reference, query)),
      ),
    )
    .handle("milestones", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => listMilestones(member, params.reference, query)),
      ),
    )
    .handle("createMilestone", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          createMilestone(member, params.reference, payload),
        ),
      ),
    )
    .handle("cycles", ({ query }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => listCycles(member, query))),
    )
    .handle("users", ({ query }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => listUsers(member, query))),
    )
    .handle("labels", ({ query }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => listLabels(member, query))),
    )
    .handle("createLabel", ({ payload }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => createLabel(member, payload))),
    );

export const issuesHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof issueGroup>,
) =>
  handlers
    .handle("queue", ({ query }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => memberQueue(member, query))),
    )
    .handle("issues", ({ query }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => listIssues(member, query))),
    )
    .handle("groups", ({ query }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => groupIssues(member, query))),
    )
    .handle("issue", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => getIssue(member, params.reference))),
    )
    .handle("createIssue", ({ payload }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => createIssue(member, payload))),
    )
    .handle("editIssue", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => editIssue(member, params.reference, payload)),
      ),
    )
    .handle("archiveIssue", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          setIssueArchived(member, params.reference, true, payload.version),
        ),
      ),
    )
    .handle("unarchiveIssue", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          setIssueArchived(member, params.reference, false, payload.version),
        ),
      ),
    )
    .handle("comments", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => issueComments(member, params.reference, query)),
      ),
    )
    .handle("createComment", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => createComment(member, params.reference, payload)),
      ),
    )
    .handle("replies", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          issueComments(member, params.reference, query, params.root),
        ),
      ),
    )
    .handle("relations", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => listRelations(member, params.reference, query)),
      ),
    )
    .handle("movePreview", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => previewMove(member, params.reference, payload)),
      ),
    )
    .handle("moveIssue", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => moveIssue(member, params.reference, payload)),
      ),
    )
    .handle("activity", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => issueTimeline(member, params.reference, query)),
      ),
    );
