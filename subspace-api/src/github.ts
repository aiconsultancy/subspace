import { createHmac, timingSafeEqual } from "node:crypto";
import { and, asc, eq, inArray, isNull, notInArray, or, sql } from "drizzle-orm";
import { Context, Effect, Option, Redacted, Schema } from "effect";
import { marked, type Token } from "marked";
import { HttpApiEndpoint, HttpApiGroup, HttpApiSchema } from "effect/http-api";
import { Database } from "./db";
import { type createGitHubClient } from "./github-app";
import { githubStatus } from "./github-poll";
import { CurrentMember, type Member } from "./identity";
import { Version } from "./initiative-contract";
import {
  addActivity,
  clearReadyForAgents,
  datesForState,
  lockIssues,
  readIssue,
  refreshTrackers,
} from "./issues";
import { BearerAuth, handleDb, ValidationErrors, versionedApi } from "./middleware";
import { byKey, page } from "./paging";
import { endpointErrors, problem, Unauthorised } from "./problems";
import { Project, projectDto, readProject } from "./projects";
import {
  githubDeploymentStatuses,
  issueAliases,
  issues,
  projectRepositories,
  projects,
  pullRequestLinks,
  pullRequests,
  states,
  users,
} from "./schema";
import {
  dateTime,
  dateTimeOrNull,
  Line,
  NullableString,
  PageQuery,
  Ref,
  Timestamp,
  Uuid,
  pageOf,
} from "./validation";

export class GithubIntegration extends Context.Service<
  GithubIntegration,
  {
    secret: Redacted.Redacted<string>;
    organisationId?: string;
    installationId?: string;
    client: ReturnType<typeof createGitHubClient> | null;
  }
>()("subspace/GithubIntegration") {}

const Repo = Schema.String.check(Schema.isPattern(/^[A-Za-z0-9-]{1,39}\/[A-Za-z0-9_.-]{1,100}$/u));
const RepoList = Schema.Struct({
  project: Ref,
  version: Version,
  items: Schema.Array(
    Schema.Struct({
      repository: Repo,
      githubRepositoryId: Schema.Null,
      url: Schema.String,
      archived: Schema.Null,
    }),
  ),
});
export const SetRepos = Schema.Struct({
  version: Version,
  repositories: Schema.Array(Repo).check(Schema.isMaxLength(50)),
});
const PullRequest = Schema.Struct({
  id: Uuid,
  repository: Schema.Struct({
    owner: Line(39),
    name: Line(100),
    fullName: Repo,
    url: Schema.String,
  }),
  number: Schema.Int,
  url: Schema.String,
  title: Schema.String,
  authorLogin: NullableString,
  state: Schema.Literals(["open", "closed", "merged"]),
  isDraft: Schema.Boolean,
  headBranch: Schema.String,
  baseBranch: Schema.String,
  referenceKind: Schema.Literals(["closes", "mentions"]),
  referenceSources: Schema.Array(Schema.String),
  referenceActive: Schema.Boolean,
  completion: Schema.String,
  mergedAt: Schema.NullOr(Timestamp),
  closedAt: Schema.NullOr(Timestamp),
  githubUpdatedAt: Timestamp,
  lastObservedAt: Timestamp,
}).annotate({ identifier: "IssuePullRequest" });

const PullObservation = Schema.Struct({
  number: Schema.Int.check(Schema.isBetween({ minimum: 1, maximum: 2147483647 })),
  title: Schema.String.check(Schema.isMaxLength(300)),
  body: Schema.NullOr(Schema.String.check(Schema.isMaxLength(100_000))),
  user: Schema.NullOr(Schema.Struct({ login: Line(80) })),
  head: Schema.Struct({ ref: Line(1024) }),
  base: Schema.Struct({ ref: Line(1024) }),
  state: Schema.Literals(["open", "closed"]),
  draft: Schema.Boolean,
  merged_at: Schema.NullOr(Schema.DateTimeUtcFromString),
  closed_at: Schema.NullOr(Schema.DateTimeUtcFromString),
  updated_at: Schema.DateTimeUtcFromString,
});
const GitHubId = Schema.Int.check(
  Schema.isBetween({ minimum: 1, maximum: Number.MAX_SAFE_INTEGER }),
);
const Installation = Schema.optional(Schema.Struct({ id: GitHubId }));
const Webhook = Schema.Struct({
  installation: Installation,
  repository: Schema.Struct({ full_name: Repo }),
  pull_request: PullObservation,
});
export const decodePull = Schema.decodeUnknownOption(PullObservation);
const decode = Schema.decodeUnknownOption(Schema.fromJsonString(Webhook));

type Db = (typeof Database)["Service"];
type Pr = typeof pullRequests.$inferSelect;
type Link = typeof pullRequestLinks.$inferSelect;

const fullUrl = (repository: string) => `https://github.com/${repository}`;

export const repoList = (member: Member, ref: string) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const { project } = yield* readProject(db, member, ref);
    const rows = yield* db
      .select()
      .from(projectRepositories)
      .where(
        and(
          eq(projectRepositories.organisationId, member.organisationId),
          eq(projectRepositories.projectId, project.id),
        ),
      )
      .orderBy(projectRepositories.repository);
    return {
      project: project.key,
      version: project.version,
      items: rows.map(({ repository }) => ({
        repository,
        githubRepositoryId: null,
        url: fullUrl(repository),
        archived: null,
      })),
    };
  });

const SingleRepo = Schema.Struct({ repository: Repo, version: Schema.optional(Version) });
const RepositoryQuery = Schema.Struct({
  repository: Repo,
  ...PageQuery,
  includeArchived: Schema.optional(Schema.Boolean),
});
type RepositoryChange =
  | typeof SetRepos.Type
  | (typeof SingleRepo.Type & { mode: "add" | "remove" });

function requireRepositoryManager(
  integration: (typeof GithubIntegration)["Service"],
  member: Member,
  project: typeof projects.$inferSelect,
) {
  if (integration.organisationId) {
    return member.organisationId === integration.organisationId && member.role === "admin"
      ? Effect.void
      : Effect.fail(
          problem(
            403,
            "Repository configuration requires an administrator of the GitHub integration organisation.",
          ),
        );
  }
  return member.role === "admin" || member.id === project.leadId
    ? Effect.void
    : Effect.fail(
        problem(
          403,
          "Repository configuration requires the project lead or an organisation admin.",
        ),
      );
}

function requireEditableRepositoryProject(project: typeof projects.$inferSelect, version?: number) {
  if (project.archivedAt) {
    return Effect.fail(problem(409, "Unarchive the project before managing repositories."));
  }
  if (version !== undefined && project.version !== version) {
    return Effect.fail(problem(409, "The project version is stale.", project.version));
  }
  return Effect.void;
}

export const setRepos = (member: Member, ref: string, payload: RepositoryChange) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const integration = yield* GithubIntegration;
    const { project: target } = yield* readProject(db, member, ref);
    yield* requireRepositoryManager(integration, member, target);
    yield* requireEditableRepositoryProject(target, payload.version);
    const requested =
      "repositories" in payload
        ? payload.repositories
        : payload.mode === "add"
          ? [payload.repository]
          : [];
    const candidates = [...new Set(requested.map((name) => name.toLowerCase()))];
    const client = integration.client;
    if (client && candidates.length) {
      const installed = yield* Effect.tryPromise((signal) =>
        client.installedRepositories(candidates, signal),
      ).pipe(Effect.mapError(() => problem(503, "The GitHub App repository list is unavailable.")));
      if (!installed)
        return yield* Effect.fail(
          problem(403, "The GitHub App installation cannot access every requested repository."),
        );
    }
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockIssues(tx, member);
        const { project } = yield* readProject(tx, member, target.id);
        yield* requireRepositoryManager(integration, member, project);
        yield* requireEditableRepositoryProject(project, payload.version);
        const current = yield* repoList(member, project.id).pipe(
          Effect.provideService(Database, tx),
        );
        const existing = current.items.map(({ repository }) => repository);
        const names =
          "repositories" in payload
            ? payload.repositories
            : payload.mode === "add"
              ? [...existing, payload.repository]
              : existing.filter((name) => name !== payload.repository.toLowerCase());
        const repositories = [...new Set(names.map((name) => name.toLowerCase()))].sort();
        if (repositories.length > 50)
          return yield* Effect.fail(problem(400, "A project can map at most 50 repositories."));
        if (repositories.join() === existing.join()) return current;
        const [saved] = yield* tx
          .update(projects)
          .set({ version: project.version + 1, updatedAt: sql`now()` })
          .where(
            and(
              eq(projects.organisationId, member.organisationId),
              eq(projects.id, project.id),
              eq(projects.version, project.version),
            ),
          )
          .returning();
        if (!saved) return yield* Effect.fail(problem(409, "The project version is stale."));
        yield* tx
          .delete(projectRepositories)
          .where(
            and(
              eq(projectRepositories.organisationId, member.organisationId),
              eq(projectRepositories.projectId, project.id),
              repositories.length
                ? notInArray(projectRepositories.repository, repositories)
                : undefined,
            ),
          );
        if (repositories.length)
          yield* tx
            .insert(projectRepositories)
            .values(
              repositories.map((repository) => ({
                organisationId: member.organisationId,
                projectId: project.id,
                repository,
              })),
            )
            .onConflictDoNothing();
        return {
          project: project.key,
          version: saved.version,
          items: repositories.map((repository) => ({
            repository,
            githubRepositoryId: null,
            url: fullUrl(repository),
            archived: null,
          })),
        };
      }),
    );
  });

function prDto(row: Pr, link: Link) {
  const [owner, name] = row.repository.split("/");
  const repository = {
    owner: owner!,
    name: name!,
    fullName: row.repository,
    url: fullUrl(row.repository),
  };
  return {
    id: row.id,
    repository,
    number: row.number,
    url: `${repository.url}/pull/${row.number}`,
    title: row.title,
    authorLogin: row.authorLogin,
    state: row.state,
    isDraft: row.isDraft,
    headBranch: row.headBranch,
    baseBranch: row.baseBranch,
    referenceKind: link.kind,
    referenceSources: link.sources,
    referenceActive: link.active,
    completion: link.completion,
    mergedAt: dateTimeOrNull(row.mergedAt),
    closedAt: dateTimeOrNull(row.closedAt),
    githubUpdatedAt: dateTime(row.githubUpdatedAt),
    lastObservedAt: dateTime(row.lastObservedAt),
  };
}

export const listPullRequests = (
  member: Member,
  reference: string,
  query: { includeInactive?: boolean; cursor?: string; limit?: number },
) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const issue = yield* readIssue(db, member, reference);
    const org = member.organisationId;
    const rows = yield* db
      .select({ pr: pullRequests, link: pullRequestLinks })
      .from(pullRequestLinks)
      .innerJoin(
        pullRequests,
        and(
          eq(pullRequests.organisationId, org),
          eq(pullRequests.id, pullRequestLinks.pullRequestId),
        ),
      )
      .where(
        and(
          eq(pullRequestLinks.organisationId, org),
          eq(pullRequestLinks.issueId, issue.id),
          query.includeInactive ? undefined : eq(pullRequestLinks.active, true),
        ),
      );
    const key = ({ pr }: (typeof rows)[number]) =>
      `${String(Number.MAX_SAFE_INTEGER - pr.githubUpdatedAt.valueOf()).padStart(16, "0")}:${pr.id}`;
    const result = yield* page(
      rows.sort(byKey(key)),
      { org, prIssue: issue.id, includeInactive: Boolean(query.includeInactive) },
      key,
      query.limit ?? 20,
      query.cursor,
    );
    return { ...result, items: result.items.map(({ pr, link }) => prDto(pr, link)) };
  });

function visible(tokens: Token[]): string {
  return tokens
    .map((token) => {
      switch (token.type) {
        case "paragraph":
        case "heading":
        case "strong":
        case "em":
        case "blockquote":
          return visible(token.tokens ?? []);
        case "link":
          return token.raw.startsWith("[") ? visible(token.tokens ?? []) : "\0";
        case "text":
          return token.tokens ? visible(token.tokens) : token.text;
        case "list":
          return token.items
            .map((item: Token & { tokens: Token[] }) => visible(item.tokens))
            .join("\0");
        case "br":
          return "\n";
        default:
          return "\0";
      }
    })
    .join("");
}

export function extractLinks(pr: Pick<typeof PullObservation.Type, "title" | "body" | "head">) {
  const links = new Map<string, { kind: Link["kind"]; sources: Set<string> }>();
  const sources = [
    ["title", visible(marked.lexer(pr.title))],
    ["body", visible(marked.lexer(pr.body ?? ""))],
    ["branch", pr.head.ref],
  ] as const;
  for (const [source, value] of sources) {
    const regex =
      source === "branch"
        ? /(?<![A-Z0-9])([A-Z][A-Z0-9]{1,9})-(\d+)(?![A-Z0-9])/giu
        : /(?<![A-Z0-9_])([A-Z][A-Z0-9]{1,9})-(\d+)(?![A-Z0-9_])/giu;
    for (const match of value.matchAll(regex)) {
      const number = Number(match[2]);
      if (number < 1 || number > 2147483647) continue;
      const identifier = `${match[1]!.toUpperCase()}-${number}`;
      const link = links.get(identifier) ?? { kind: "mentions", sources: new Set<string>() };
      if (
        source !== "branch" &&
        /\b(?:close[sd]?|fix(?:es|ed)?|resolve[sd]?):?\s+$/iu.test(value.slice(0, match.index))
      ) {
        link.kind = "closes";
      }
      link.sources.add(source);
      links.set(identifier, link);
      if (links.size >= 200) return links;
    }
  }
  return links;
}

function completeIssue(db: Db, issue: typeof issues.$inferSelect, pr: Pr) {
  return Effect.gen(function* () {
    if (issue.type === "tracker") return "skippedTracker" as const;
    const [project] = yield* db
      .select({ archivedAt: projects.archivedAt })
      .from(projects)
      .where(
        and(eq(projects.organisationId, issue.organisationId), eq(projects.id, issue.projectId)),
      );
    if (issue.archivedAt || project?.archivedAt) return "skippedArchived" as const;
    const [state] = yield* db
      .select({ type: states.type })
      .from(states)
      .where(and(eq(states.organisationId, issue.organisationId), eq(states.id, issue.stateId)));
    if (state && ["completed", "canceled", "duplicate"].includes(state.type))
      return "skippedTerminal" as const;
    const [completed] = yield* db
      .select({ id: states.id })
      .from(states)
      .where(
        and(
          eq(states.organisationId, issue.organisationId),
          eq(states.projectId, issue.projectId),
          eq(states.type, "completed"),
        ),
      )
      .orderBy(asc(states.position), asc(states.id))
      .limit(1);
    if (!completed) return "skippedNoState" as const;
    const cleared = clearReadyForAgents(issue, true);
    yield* db
      .update(issues)
      .set({
        stateId: completed.id,
        ...datesForState("completed", issue),
        ...cleared.set,
        version: issue.version + 1,
        updatedAt: sql`now()`,
        lastActivityAt: sql`now()`,
      })
      .where(and(eq(issues.organisationId, issue.organisationId), eq(issues.id, issue.id)));
    yield* addActivity(
      db,
      { organisationId: issue.organisationId, id: null },
      issue.id,
      "issue.changed",
      {
        fields: ["stateId", ...cleared.fields],
        pullRequest: { repository: pr.repository, number: pr.number },
      },
    );
    return "completed" as const;
  });
}

function mappedProjects(db: Db, repository: string, organisationId: string) {
  return db
    .select()
    .from(projectRepositories)
    .where(
      and(
        eq(projectRepositories.repository, repository),
        eq(projectRepositories.organisationId, organisationId),
      ),
    )
    .orderBy(projectRepositories.projectId);
}

function targetsFor(db: Db, org: string, projectIds: string[], identifiers: string[]) {
  if (!projectIds.length || !identifiers.length) {
    return Effect.succeed([] as { issue: typeof issues.$inferSelect; alias: string | null }[]);
  }
  return db
    .select({ issue: issues, alias: issueAliases.identifier })
    .from(issues)
    .leftJoin(
      issueAliases,
      and(
        eq(issueAliases.organisationId, org),
        eq(issueAliases.issueId, issues.id),
        inArray(issueAliases.identifier, identifiers),
      ),
    )
    .where(
      and(
        eq(issues.organisationId, org),
        inArray(issues.projectId, projectIds),
        or(inArray(issues.identifier, identifiers), inArray(issueAliases.identifier, identifiers)),
      ),
    );
}

function observePull(db: Db, org: string, repo: string, incoming: typeof PullObservation.Type) {
  return Effect.gen(function* () {
    const mapped = yield* mappedProjects(db, repo, org);
    if (!mapped.length) return;
    const priorRows = yield* db
      .select()
      .from(pullRequests)
      .where(
        and(
          eq(pullRequests.organisationId, org),
          eq(pullRequests.repository, repo),
          eq(pullRequests.number, incoming.number),
        ),
      );
    const prior = priorRows[0];
    const githubUpdatedAt = new Date(incoming.updated_at.epochMilliseconds);
    const mergedAt = incoming.merged_at ? new Date(incoming.merged_at.epochMilliseconds) : null;
    if (prior && (prior.githubUpdatedAt > githubUpdatedAt || (prior.mergedAt && !mergedAt))) return;
    const fields = {
      title: incoming.title,
      authorLogin: incoming.user?.login ?? null,
      state: mergedAt ? ("merged" as const) : incoming.state,
      isDraft: incoming.draft,
      headBranch: incoming.head.ref,
      baseBranch: incoming.base.ref,
      mergedAt,
      closedAt: incoming.closed_at ? new Date(incoming.closed_at.epochMilliseconds) : null,
      githubUpdatedAt,
      lastObservedAt: new Date(),
    };
    const [pr] = yield* db
      .insert(pullRequests)
      .values({
        organisationId: org,
        repository: repo,
        number: incoming.number,
        ...fields,
      })
      .onConflictDoUpdate({
        target: [pullRequests.organisationId, pullRequests.repository, pullRequests.number],
        set: fields,
      })
      .returning();
    if (!pr) return;
    const previous = yield* db
      .select()
      .from(pullRequestLinks)
      .where(
        and(eq(pullRequestLinks.organisationId, org), eq(pullRequestLinks.pullRequestId, pr.id)),
      );
    const byIssue = new Map(previous.map((link) => [link.issueId, link]));
    const wanted = extractLinks(incoming);
    const mappedAt = new Map(mapped.map((row) => [row.projectId, row.createdAt]));
    const targets = yield* targetsFor(db, org, [...mappedAt.keys()], [...wanted.keys()]);
    const parents: (string | null)[] = [];
    const actor = { organisationId: org, id: null };
    for (const [id, matches] of Map.groupBy(targets, ({ issue }) => issue.id)) {
      const issue = matches[0]!.issue;
      const links = [
        issue.identifier,
        ...matches.flatMap(({ alias }) => (alias ? [alias] : [])),
      ].flatMap((identifier) => wanted.get(identifier) ?? []);
      const kind = links.some((link) => link.kind === "closes")
        ? ("closes" as const)
        : ("mentions" as const);
      const sources = [...new Set(links.flatMap((link) => [...link.sources]))].sort();
      const old = byIssue.get(id);
      const eligible = mergedAt && !prior?.mergedAt && mergedAt >= mappedAt.get(issue.projectId)!;
      const completion =
        eligible && kind === "closes" && (!old || old.completion === "none")
          ? yield* completeIssue(db, issue, pr)
          : (old?.completion ?? "none");
      if (completion === "completed" && old?.completion !== "completed")
        parents.push(issue.parentId);
      const facts = { kind, sources, active: true, completion };
      yield* db
        .insert(pullRequestLinks)
        .values({ organisationId: org, issueId: id, pullRequestId: pr.id, ...facts })
        .onConflictDoUpdate({
          target: [
            pullRequestLinks.organisationId,
            pullRequestLinks.issueId,
            pullRequestLinks.pullRequestId,
          ],
          set: facts,
        });
      if (
        !old ||
        !old.active ||
        old.kind !== kind ||
        old.sources.join() !== sources.join() ||
        prior?.state !== pr.state ||
        prior?.title !== pr.title ||
        prior?.isDraft !== pr.isDraft
      ) {
        yield* addActivity(db, actor, id, old ? "pull-request.updated" : "pull-request.linked", {
          repository: repo,
          number: pr.number,
          state: pr.state,
          completion,
        });
      }
      byIssue.delete(id);
    }
    for (const old of byIssue.values()) {
      if (!old.active) continue;
      yield* db
        .update(pullRequestLinks)
        .set({ active: false })
        .where(
          and(
            eq(pullRequestLinks.organisationId, org),
            eq(pullRequestLinks.issueId, old.issueId),
            eq(pullRequestLinks.pullRequestId, pr.id),
          ),
        );
      yield* addActivity(db, actor, old.issueId, "pull-request.unlinked", {
        repository: repo,
        number: pr.number,
      });
    }
    yield* refreshTrackers(db, actor, parents);
  });
}

export function recordPullRequest(db: Db, event: typeof Webhook.Type, organisationId: string) {
  return db.transaction((tx) =>
    Effect.gen(function* () {
      const repository = event.repository.full_name.toLowerCase();
      yield* lockIssues(tx, { organisationId });
      yield* observePull(tx, organisationId, repository, event.pull_request);
    }),
  );
}

const DeploymentState = Schema.Literals([
  "error",
  "failure",
  "inactive",
  "in_progress",
  "queued",
  "pending",
  "success",
]);
const DeploymentWebhook = Schema.Struct({
  installation: Installation,
  repository: Schema.Struct({ full_name: Repo }),
  deployment: Schema.Struct({
    id: GitHubId,
    sha: Schema.String.check(Schema.isPattern(/^[0-9a-f]{40}$/u)),
    environment: Line(255),
  }),
  deployment_status: Schema.Struct({
    id: GitHubId,
    state: DeploymentState,
    created_at: Schema.DateTimeUtcFromString,
    environment: Schema.optional(Line(255)),
  }),
});
export const decodeDeployment = Schema.decodeUnknownOption(DeploymentWebhook);
const decodeDeploymentBody = Schema.decodeUnknownOption(Schema.fromJsonString(DeploymentWebhook));
const DeploymentStatus = Schema.Struct({
  repository: Repo,
  deploymentId: GitHubId,
  statusId: GitHubId,
  sha: Schema.String,
  environment: Line(255),
  state: DeploymentState,
  occurredAt: Timestamp,
});

export function recordDeployment(
  db: Db,
  event: typeof DeploymentWebhook.Type,
  organisationId: string,
) {
  return Effect.gen(function* () {
    const repository = event.repository.full_name.toLowerCase();
    const mapped = yield* mappedProjects(db, repository, organisationId);
    if (!mapped.length) return;
    const status = event.deployment_status;
    yield* db
      .insert(githubDeploymentStatuses)
      .values({
        organisationId,
        repository,
        deploymentId: event.deployment.id,
        statusId: status.id,
        environment: status.environment ?? event.deployment.environment,
        sha: event.deployment.sha,
        state: status.state,
        occurredAt: new Date(status.created_at.epochMilliseconds),
      })
      .onConflictDoNothing();
  });
}

function listDeploymentStatuses(
  member: Member,
  reference: string,
  query: { cursor?: string; limit?: number },
) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const { project } = yield* readProject(db, member, reference);
    const rows = yield* db
      .select({ status: githubDeploymentStatuses })
      .from(githubDeploymentStatuses)
      .innerJoin(
        projectRepositories,
        and(
          eq(projectRepositories.organisationId, member.organisationId),
          eq(projectRepositories.projectId, project.id),
          eq(projectRepositories.repository, githubDeploymentStatuses.repository),
        ),
      )
      .where(eq(githubDeploymentStatuses.organisationId, member.organisationId));
    const key = ({ status }: (typeof rows)[number]) =>
      `${String(Number.MAX_SAFE_INTEGER - status.occurredAt.valueOf()).padStart(16, "0")}:${status.repository}:${status.deploymentId}:${status.statusId}`;
    const result = yield* page(
      rows.sort(byKey(key)),
      { org: member.organisationId, deployments: project.id },
      key,
      query.limit,
      query.cursor,
    );
    return {
      ...result,
      items: result.items.map(({ status }) => ({
        ...status,
        state: status.state as typeof DeploymentState.Type,
        occurredAt: dateTime(status.occurredAt),
      })),
    };
  });
}

export function repositoryProjects(member: Member, input: typeof RepositoryQuery.Type) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const org = member.organisationId;
    const repo = input.repository.toLowerCase();
    const records = yield* db
      .select({ project: projects, lead: users })
      .from(projectRepositories)
      .innerJoin(
        projects,
        and(eq(projects.organisationId, org), eq(projects.id, projectRepositories.projectId)),
      )
      .innerJoin(users, and(eq(users.organisationId, org), eq(users.id, projects.leadId)))
      .where(
        and(
          eq(projectRepositories.organisationId, org),
          eq(projectRepositories.repository, repo),
          input.includeArchived ? undefined : isNull(projects.archivedAt),
        ),
      );
    const key = ({ project }: (typeof records)[number]) => project.key;
    const result = yield* page(
      records.sort(byKey(key)),
      { org, repo, includeArchived: Boolean(input.includeArchived) },
      key,
      input.limit,
      input.cursor,
    );
    return { ...result, items: result.items.map(({ project, lead }) => projectDto(project, lead)) };
  });
}

const pullQuery = { ...PageQuery, includeInactive: Schema.optional(Schema.Boolean) };
const params = { reference: Ref };
export const githubGroup = HttpApiGroup.make("github")
  .add(
    HttpApiEndpoint.get("pullRequests", "/issues/:reference/pull-requests", {
      params,
      query: pullQuery,
      success: pageOf(PullRequest),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("githubStatus", "/integrations/github/status", {
      success: Schema.Struct({
        configured: Schema.Boolean,
        lastPollStartedAt: Schema.NullOr(Timestamp),
        lastPollFinishedAt: Schema.NullOr(Timestamp),
        lastErrorCategory: NullableString,
        repositoryCount: Schema.Int,
        pullRequestCount: Schema.Int,
        lastPollErrorCount: Schema.Int,
        totalErrorCount: Schema.Int,
        stalledRepositories: Schema.Array(Schema.Unknown),
      }),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("deploymentStatuses", "/projects/:reference/deployment-statuses", {
      params,
      query: PageQuery,
      success: pageOf(DeploymentStatus),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("repositories", "/projects/:reference/repositories", {
      params,
      success: RepoList,
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("repositoryProjects", "/repositories/projects", {
      query: RepositoryQuery,
      success: pageOf(Project),
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("addRepository", "/projects/:reference/repositories", {
      params,
      payload: SingleRepo,
      success: RepoList,
      error: endpointErrors,
    }),
    HttpApiEndpoint.delete("removeRepository", "/projects/:reference/repositories", {
      params,
      payload: SingleRepo,
      success: RepoList,
      error: endpointErrors,
    }),
    HttpApiEndpoint.put("setRepositories", "/projects/:reference/repositories", {
      params,
      payload: SetRepos,
      success: RepoList,
      error: endpointErrors,
    }),
  )
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const githubEventGroup = HttpApiGroup.make("githubWebhook")
  .add(
    HttpApiEndpoint.post("githubWebhook", "/integrations/github/webhook", {
      success: HttpApiSchema.NoContent,
      error: [...endpointErrors, Unauthorised],
    }),
  )
  .prefix(versionedApi);

export const githubHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof githubGroup>,
) =>
  handlers
    .handle("pullRequests", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          listPullRequests(member, params.reference, query),
        ),
      ),
    )
    .handle("githubStatus", () =>
      handleDb(Effect.flatMap(CurrentMember, (member) => githubStatus(member.organisationId))),
    )
    .handle("deploymentStatuses", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          listDeploymentStatuses(member, params.reference, query),
        ),
      ),
    )
    .handle("repositories", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => repoList(member, params.reference))),
    )
    .handle("repositoryProjects", ({ query }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => repositoryProjects(member, query))),
    )
    .handle("addRepository", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          setRepos(member, params.reference, { ...payload, mode: "add" }),
        ),
      ),
    )
    .handle("removeRepository", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          setRepos(member, params.reference, { ...payload, mode: "remove" }),
        ),
      ),
    )
    .handle("setRepositories", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => setRepos(member, params.reference, payload)),
      ),
    );

export const githubEventHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof githubEventGroup>,
) =>
  handlers.handle("githubWebhook", ({ request }) =>
    handleDb(
      Effect.gen(function* () {
        const integration = yield* GithubIntegration;
        const secret = Redacted.value(integration.secret);
        const { organisationId, installationId } = integration;
        if (!secret || !organisationId)
          return yield* Effect.fail(problem(404, "Webhook unavailable."));
        const signature = request.headers["x-hub-signature-256"];
        if (!signature || !/^sha256=[0-9a-f]{64}$/u.test(signature))
          return yield* Effect.fail(problem(401, "Webhook signature is required."));
        if (Number(request.headers["content-length"]) > 1_000_000)
          return yield* Effect.fail(problem(400, "Webhook body is too large."));
        const raw = yield* request.arrayBuffer.pipe(
          Effect.mapError(() => problem(400, "Webhook body is unavailable.")),
        );
        if (raw.byteLength > 1_000_000)
          return yield* Effect.fail(problem(400, "Webhook body is too large."));
        const bytes = Buffer.from(raw);
        const hmac = createHmac("sha256", secret).update(bytes).digest();
        if (!timingSafeEqual(Buffer.from(signature.slice(7), "hex"), hmac))
          return yield* Effect.fail(problem(401, "Webhook signature is invalid."));
        const type = request.headers["x-github-event"];
        const body = bytes.toString("utf8");
        if (type === "pull_request") {
          const event = decode(body);
          if (Option.isNone(event))
            return yield* Effect.fail(problem(400, "Invalid pull request event."));
          if (installationId && String(event.value.installation?.id) !== installationId)
            return yield* Effect.fail(
              problem(403, "Webhook installation does not match this integration."),
            );
          yield* recordPullRequest(yield* Database, event.value, organisationId);
        }
        if (type === "deployment_status") {
          const event = decodeDeploymentBody(body);
          if (Option.isNone(event))
            return yield* Effect.fail(problem(400, "Invalid deployment status event."));
          if (installationId && String(event.value.installation?.id) !== installationId)
            return yield* Effect.fail(
              problem(403, "Webhook installation does not match this integration."),
            );
          yield* recordDeployment(yield* Database, event.value, organisationId);
        }
      }),
    ),
  );
