import { and, eq, sql } from "drizzle-orm";
import { Context, Effect, Option, Schedule } from "effect";
import { Database } from "./db";
import { decodePull, recordPullRequest } from "./github";
import { type createGitHubClient, type githubConfig } from "./github-app";
import { githubPollCheckpoints, projectRepositories, projects, pullRequests } from "./schema";
import { dateTimeOrNull } from "./validation";

type Settings = ReturnType<typeof githubConfig>;
type Client = ReturnType<typeof createGitHubClient>;
type Db = (typeof Database)["Service"];

export class GithubPollState extends Context.Service<
  GithubPollState,
  ReturnType<typeof githubPollState>
>()("subspace/GithubPollState") {}

export function githubPollState(settings: Settings) {
  return {
    configured: Boolean(settings.app),
    organisationId: settings.organisationId,
    started: null as Date | null,
    finished: null as Date | null,
    errors: new Map<string, number>(),
    lastErrors: new Map<string, number>(),
  };
}

export function githubStatus(organisationId: string) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const state = yield* GithubPollState;
    const scoped = !state.organisationId || state.organisationId === organisationId;
    const [repository] = yield* db
      .select({
        total: sql<number>`count(distinct ${projectRepositories.repository})::int`,
      })
      .from(projectRepositories)
      .where(eq(projectRepositories.organisationId, organisationId));
    const [pulls] = yield* db
      .select({
        total: sql<number>`count(*)::int`,
      })
      .from(pullRequests)
      .where(eq(pullRequests.organisationId, organisationId));
    const lastPollErrorCount = state.lastErrors.get(organisationId) ?? 0;
    return {
      configured: scoped && state.configured,
      lastPollStartedAt: dateTimeOrNull(scoped ? state.started : null),
      lastPollFinishedAt: dateTimeOrNull(scoped ? state.finished : null),
      lastErrorCategory: lastPollErrorCount ? "repositoryPollFailed" : null,
      lastPollErrorCount,
      totalErrorCount: state.errors.get(organisationId) ?? 0,
      repositoryCount: repository?.total ?? 0,
      pullRequestCount: pulls?.total ?? 0,
      stalledRepositories: [],
    };
  });
}

function pollRepository(db: Db, client: Client, organisationId: string, repository: string) {
  return Effect.gen(function* () {
    const [previous] = yield* db
      .select()
      .from(githubPollCheckpoints)
      .where(
        and(
          eq(githubPollCheckpoints.organisationId, organisationId),
          eq(githubPollCheckpoints.repository, repository),
        ),
      );
    const started = Date.now();
    const cutoff = previous?.observedThrough.valueOf() ?? started - 14 * 86400_000;
    let highest = cutoff;
    let finished = false;
    for (let page = 1; page <= 100 && !finished; page++) {
      const result = yield* Effect.tryPromise((signal) => client.pulls(repository, page, signal));
      for (const item of result.rows) {
        const pr = decodePull(item);
        if (Option.isNone(pr)) continue;
        const updated = pr.value.updated_at.epochMilliseconds;
        if (updated < cutoff) {
          finished = true;
          break;
        }
        yield* recordPullRequest(
          db,
          { repository: { full_name: repository }, pull_request: pr.value },
          organisationId,
        );
        highest = Math.max(highest, Math.min(updated, started));
      }
      if (!result.next) finished = true;
    }
    if (!finished) return yield* Effect.fail(new Error("GitHub pull pagination limit"));
    const observedThrough = new Date(highest);
    yield* db
      .insert(githubPollCheckpoints)
      .values({ organisationId, repository, observedThrough })
      .onConflictDoUpdate({
        target: [githubPollCheckpoints.organisationId, githubPollCheckpoints.repository],
        set: {
          observedThrough: sql`greatest(${githubPollCheckpoints.observedThrough}, ${observedThrough.toISOString()}::timestamptz)`,
        },
      });
  });
}

export function pollGitHub(settings: Settings, client: Client | null) {
  const { organisationId } = settings;
  if (!client || !organisationId) return Effect.void;
  return Effect.gen(function* () {
    const db = yield* Database;
    const state = yield* GithubPollState;
    state.started = new Date();
    state.lastErrors.clear();
    const mappings = yield* db
      .selectDistinct({
        repository: projectRepositories.repository,
      })
      .from(projectRepositories)
      .innerJoin(
        projects,
        and(
          eq(projects.id, projectRepositories.projectId),
          eq(projects.organisationId, projectRepositories.organisationId),
          sql`${projects.archivedAt} is null`,
        ),
      )
      .where(eq(projectRepositories.organisationId, organisationId));
    for (const { repository } of mappings) {
      yield* pollRepository(db, client, organisationId, repository).pipe(
        Effect.catch(() =>
          Effect.sync(() => {
            state.lastErrors.set(organisationId, (state.lastErrors.get(organisationId) ?? 0) + 1);
            state.errors.set(organisationId, (state.errors.get(organisationId) ?? 0) + 1);
          }),
        ),
      );
    }
    state.finished = new Date();
  }).pipe(
    Effect.catch(() => Effect.logWarning("GitHub polling could not query its project mappings.")),
    Effect.repeat(Schedule.spaced(`${settings.intervalSeconds} seconds`)),
    Effect.forkScoped,
  );
}
