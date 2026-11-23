import { and, eq, isNotNull, or, sql } from "drizzle-orm";
import { Context, Effect, Layer, type Types } from "effect";
import { HttpRouter, HttpServerRequest, HttpServerResponse } from "effect/http";
import { type Config } from "./config";
import { Database } from "./db";
import { CurrentMember, type Member } from "./identity";
import { BearerAuth } from "./middleware";
import { problem } from "./problems";
import { fleets, organisations, ships, users } from "./schema";

/**
 * The agent listener: a second, optional listener on which a request names its registered agent
 * in this header and needs no bearer token. It serves no sign-in route and no administrator
 * route, and resolves only active agent records, which are never administrators.
 */
export const agentHeader = "x-subspace-agent";
export const agentHeaderName = "X-Subspace-Agent";

/** An agent name or IRC nick, as registration allows them. */
const agentReference = /^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$/u;

/**
 * Routes that need an organisation administrator. The agent listener does not serve them at
 * all, as well as refusing them in their handlers, which never grant an agent the admin role.
 */
export const adminOnlyEndpoints = {
  fleets: ["registerFleet", "editFleet"],
  initiatives: ["createInitiative", "configureChannels", "assignFleet"],
  agents: ["registerAgent", "editAgent", "deactivateAgent", "reactivateAgent"],
  github: ["githubStatus"],
} as const;

/** Groups the agent listener does not serve: the signed GitHub webhook. */
export const agentListenerOmits = ["githubWebhook"] as const;

/** MCP tools that need an organisation administrator, so the agent listener does not offer them. */
export const adminOnlyTools: ReadonlySet<string> = new Set([
  "subspace_create_fleet",
  "subspace_update_fleet",
]);

export type AgentListener = NonNullable<Config["agent"]>;

type Candidate = Member;

export const unknownAgentDetail = `Name a registered agent in the ${agentHeaderName} header.`;

/**
 * Chooses the one agent a header names from the agents whose name or IRC nick matches it,
 * ignoring case. No match, or more than one, is refused, as is an inactive agent.
 */
export function chooseAgent(
  candidates: readonly Candidate[],
  reference: string | undefined,
): Effect.Effect<Member, ReturnType<typeof problem<401>> | ReturnType<typeof problem<403>>> {
  if (reference === undefined || reference.trim() === "")
    return Effect.fail(
      problem(401, `Identify the agent with the ${agentHeaderName} header on this listener.`),
    );
  if (!agentReference.test(reference)) return Effect.fail(problem(401, unknownAgentDetail));
  const agents = candidates.filter((row) => row.agentRank !== null);
  if (agents.length === 0) return Effect.fail(problem(401, unknownAgentDetail));
  if (agents.length > 1)
    return Effect.fail(
      problem(401, `'${reference}' names more than one agent. Use the agent's registered name.`),
    );
  const agent = agents[0]!;
  if (!agent.active)
    return Effect.fail(
      problem(403, `Agent '${agent.displayName}' is deactivated. Ask a human to reactivate it.`),
    );
  return Effect.succeed(agent);
}

/** The agent records in the listener's organisation whose name or IRC nick matches, ignoring case. */
export const findAgents = (listener: AgentListener, reference: string) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const lower = reference.toLowerCase();
    const rows = yield* db
      .select({ member: users, fleet: fleets.slug, ship: ships.name })
      .from(users)
      .innerJoin(organisations, eq(users.organisationId, organisations.id))
      .leftJoin(fleets, eq(fleets.id, users.agentFleetId))
      .leftJoin(ships, eq(ships.id, users.agentShipId))
      .where(
        and(
          eq(organisations.provider, listener.provider),
          eq(organisations.externalId, listener.externalOrganisation),
          isNotNull(users.agentRank),
          or(
            sql`lower(${users.displayName}) = ${lower}`,
            sql`lower(${users.agentIrcNick}) = ${lower}`,
          ),
        ),
      )
      .limit(3);
    return rows.map((row): Member => ({
      ...row.member,
      agentFleet: row.fleet ?? row.member.agentFleet,
      agentShip: row.ship ?? row.member.agentShip,
    }));
  });

/** Looks up the candidate agents for a header value. Tests replace it. */
export class AgentDirectory extends Context.Service<
  AgentDirectory,
  { readonly find: (reference: string) => Effect.Effect<readonly Member[], unknown> }
>()("subspace/AgentDirectory") {}

export const agentDirectory = (listener: AgentListener) =>
  Layer.effect(
    AgentDirectory,
    Effect.map(Database, (db) =>
      AgentDirectory.of({
        find: (reference) =>
          findAgents(listener, reference).pipe(Effect.provideService(Database, db)),
      }),
    ),
  );

/** How long an update stream on the agent listener runs before the agent is checked again. */
const identityLifetimeSeconds = 3600;

/**
 * The agent listener's authentication: the header names the agent, and the bearer credential,
 * if any, is ignored. It provides the same CurrentMember as bearer sign-in, so every read and
 * write is attributed to the agent and keeps every rule that applies to agents.
 */
export const agentAuth = Layer.effect(
  BearerAuth,
  Effect.map(AgentDirectory, (directory) =>
    BearerAuth.of({
      bearer: (next) =>
        Effect.gen(function* () {
          const request = yield* HttpServerRequest.HttpServerRequest;
          const reference = request.headers[agentHeader];
          const candidates =
            reference && agentReference.test(reference)
              ? yield* directory
                  .find(reference)
                  .pipe(Effect.mapError(() => problem(500, "The agent could not be resolved.")))
              : [];
          const agent = yield* chooseAgent(candidates, reference);
          return yield* next.pipe(
            Effect.provideService(CurrentMember, {
              ...agent,
              tokenExpiresAt: Math.floor(Date.now() / 1000) + identityLifetimeSeconds,
            }),
            Effect.map(HttpServerResponse.setHeader("Cache-Control", "private, no-store")),
          );
        }),
    }),
  ),
);

type Endpoint = { readonly method: string; readonly path: string };
type Groups = Record<string, { readonly endpoints: Record<string, Endpoint> }>;

function routePattern(path: string) {
  const source = path
    .split("/")
    .map((segment) =>
      segment.startsWith(":") ? "[^/]+" : segment.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&"),
    )
    .join("/");
  return new RegExp(`^${source}/?$`, "u");
}

/** The method and path pattern of every route the agent listener does not serve. */
export function refusedRoutes(groups: Groups) {
  const routes: { method: string; pattern: RegExp; path: string }[] = [];
  for (const [group, names] of Object.entries(adminOnlyEndpoints)) {
    for (const name of names) {
      const endpoint = groups[group]?.endpoints[name];
      if (!endpoint) throw new Error(`The admin-only endpoint ${group}.${name} does not exist.`);
      routes.push({
        method: endpoint.method,
        pattern: routePattern(endpoint.path),
        path: endpoint.path,
      });
    }
  }
  return routes;
}

/**
 * Answers 404 for the administrator routes on the agent listener before any handler runs, with
 * the API's usual problem document.
 */
export function agentRouteFilter(groups: Groups) {
  const refused = refusedRoutes(groups);
  return HttpRouter.middleware<Types.unhandled, HttpServerRequest.HttpServerRequest, true>(
    (next) =>
      Effect.gen(function* () {
        const request = yield* HttpServerRequest.HttpServerRequest;
        const path = request.url.split("?", 1)[0]!;
        if (refused.some((route) => route.method === request.method && route.pattern.test(path)))
          return HttpServerResponse.jsonUnsafe(
            problem(404, "The agent listener does not serve administrator routes."),
            { status: 404, contentType: "application/problem+json" },
          );
        return yield* next;
      }),
    { global: true },
  );
}
