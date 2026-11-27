import { and, asc, desc, eq, isNotNull, or, sql } from "drizzle-orm";
import { Context, Effect, Layer, type Types } from "effect";
import { HttpRouter, HttpServerRequest, HttpServerResponse } from "effect/http";
import { type Config } from "./config";
import { Database } from "./db";
import { CurrentMember, type Member, signedInAs } from "./identity";
import { BearerAuth } from "./middleware";
import { problem } from "./problems";
import { fleets, organisations, ships, users } from "./schema";
import { canonicalName, usernameKey } from "./usernames";

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
 * ignoring case: the one active match. No match or more than one active match is refused (401),
 * as is a name that matches only deactivated agents (403).
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
  // A deactivated record that shares the name or nick never hides the one active agent.
  const active = agents.filter((row) => row.active);
  if (active.length > 1)
    return Effect.fail(
      problem(401, `'${reference}' names more than one agent. Use the agent's registered name.`),
    );
  if (active.length === 1) return Effect.succeed(active[0]!);
  if (agents.length > 1)
    return Effect.fail(
      problem(
        403,
        `Every agent '${reference}' names is deactivated. Ask a human to reactivate one.`,
      ),
    );
  const agent = agents[0]!;
  return Effect.fail(
    problem(403, `Agent '${agent.displayName}' is deactivated. Ask a human to reactivate it.`),
  );
}

/**
 * The query for the agent records in the listener's organisation whose name or IRC nick matches,
 * ignoring case, active records first. Agent names and IRC nicks are each unique in an
 * organisation, so at most two records match; the order means no deactivated record can ever
 * take the place of the active one under the limit.
 */
export const agentLookup = (
  db: Pick<(typeof Database)["Service"], "select">,
  listener: AgentListener,
  reference: string,
) => {
  // The same ASCII comparison as usernames and Ergo, whatever the database locale.
  const key = canonicalName(reference);
  return db
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
          sql`${usernameKey(users.displayName)} = ${key}`,
          sql`${usernameKey(users.agentIrcNick)} = ${key}`,
        ),
      ),
    )
    .orderBy(desc(users.active), asc(users.id))
    .limit(3);
};

/** The agent records a header value names, active ones first. */
export const findAgents = (listener: AgentListener, reference: string) =>
  Effect.gen(function* () {
    const rows = yield* agentLookup(yield* Database, listener, reference);
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
              ...signedInAs(agent, "provisioned"),
              tokenExpiresAt: Math.floor(Date.now() / 1000) + identityLifetimeSeconds,
            }),
            Effect.map(HttpServerResponse.setHeader("Cache-Control", "private, no-store")),
          );
        }),
    }),
  ),
);

type Endpoint = { readonly method: string; readonly path: string };
type Groups = Record<
  string,
  { readonly key: string; readonly endpoints: Record<string, Endpoint> }
>;

/** The method and declared path of every administrator route, which the agent listener lacks. */
export function adminRoutes(groups: Groups) {
  return Object.entries(adminOnlyEndpoints).flatMap(([group, names]) =>
    names.map((name) => {
      const endpoint = groups[group]?.endpoints[name];
      if (!endpoint) throw new Error(`The admin-only endpoint ${group}.${name} does not exist.`);
      return { method: endpoint.method, path: endpoint.path };
    }),
  );
}

type GroupService = {
  readonly routes: readonly { readonly method: string; readonly path: string }[];
  readonly handlers: ReadonlyMap<string, unknown>;
};

/**
 * The handler layers without the administrator endpoints, so the agent listener's router never
 * registers them: they answer 404 however the path is spelt. Each administrator endpoint is
 * matched to its route by method and path and to its handler by name; if either is not found
 * exactly once, building the listener fails rather than keeping the route.
 */
export function withoutAdminEndpoints<A, E, R>(
  layer: Layer.Layer<A, E, R>,
  groups: Groups,
): Layer.Layer<A, E, R> {
  return Layer.effectContext(
    Effect.map(Layer.build(layer), (context) => {
      const services = new Map(context.mapUnsafe);
      for (const [group, names] of Object.entries(adminOnlyEndpoints)) {
        const key = groups[group]?.key;
        const service = key ? (services.get(key) as GroupService | undefined) : undefined;
        if (!key || !service) throw new Error(`The handlers for group ${group} are missing.`);
        const omitted = names.map((name) => ({ name, endpoint: groups[group]!.endpoints[name]! }));
        const isOmitted = (route: { method: string; path: string }) =>
          omitted.some(
            ({ endpoint }) => endpoint.method === route.method && endpoint.path === route.path,
          );
        for (const { name, endpoint } of omitted) {
          const matches = service.routes.filter(
            (route) => route.method === endpoint.method && route.path === endpoint.path,
          ).length;
          if (matches !== 1 || !service.handlers.has(name))
            throw new Error(`The admin-only endpoint ${group}.${name} has no single route.`);
        }
        const routes = service.routes.filter((route) => !isOmitted(route));
        const handlers = new Map(
          [...service.handlers].filter(([name]) => !(names as readonly string[]).includes(name)),
        );
        if (routes.length !== handlers.size)
          throw new Error(`The routes of group ${group} do not match its handlers.`);
        services.set(key, { ...service, routes, handlers });
      }
      return Context.makeUnsafe(services) as Context.Context<A>;
    }),
  );
}

const loopbackHost = /^(?:127\.0\.0\.1|localhost|\[::1\])(?::[0-9]{1,5})?$/u;

/**
 * Refuses, before any route runs, a request from a browser page (any Origin) and a request that
 * names a host other than loopback, so a page that rebinds its own domain to this address cannot
 * act as an agent. Every client the fleet installer configures connects to 127.0.0.1 directly,
 * and Docker's loopback publishing passes the Host header through unchanged.
 */
export const agentRequestGuard = HttpRouter.middleware<
  Types.unhandled,
  HttpServerRequest.HttpServerRequest,
  true
>(
  (next) =>
    Effect.gen(function* () {
      const request = yield* HttpServerRequest.HttpServerRequest;
      const refuse = (detail: string) =>
        HttpServerResponse.jsonUnsafe(problem(403, detail), {
          status: 403,
          contentType: "application/problem+json",
        });
      if (request.headers.origin !== undefined)
        return refuse("The agent listener accepts native clients only, without an Origin.");
      if (!loopbackHost.test(request.headers.host ?? ""))
        return refuse("Reach the agent listener at 127.0.0.1, localhost or [::1].");
      return yield* next;
    }),
  { global: true },
);
