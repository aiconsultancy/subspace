import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { Effect, Exit, Layer, Scope } from "effect";
import { AgentDirectory, adminRoutes } from "../src/agent-listener";
import { agentApi, agentHandlerLayers, server, type Resources } from "../src/api";
import { BearerAuth, validationErrors } from "../src/middleware";
import { config } from "../src/config";
import { Database } from "../src/db";
import { GithubIntegration } from "../src/github";
import { GithubPollState, githubPollState } from "../src/github-poll";
import { type Member } from "../src/identity";
import { Storage } from "../src/storage";
import { UpdateHub } from "../src/updates";

// Builds the process's real two-listener composition, `server()`, on ephemeral loopback ports
// with a stub database, and checks that each port serves only its own routes.

function freePort() {
  const probe = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response() });
  const { port } = probe;
  probe.stop(true);
  return port!;
}

const created = new Date("2026-11-02T09:00:00.000Z");
const captain: Member = {
  id: "00000000-0000-4000-8000-000000000011",
  organisationId: "00000000-0000-4000-8000-000000000001",
  subject: "orion@agents.example.com",
  email: null,
  displayName: "Orion",
  role: "member",
  active: true,
  agentRank: "captain",
  agentFleet: "alpha",
  agentFleetId: "00000000-0000-4000-8000-0000000000a1",
  agentShipId: null,
  agentShip: null,
  agentModel: "example-model",
  agentIrcNick: "orion",
  username: "orion",
  registeredById: "00000000-0000-4000-8000-0000000000ff",
  deactivatedById: null,
  deactivatedAt: null,
  version: 1,
  createdAt: created,
  updatedAt: created,
};

/** Every database call fails as a query would, so a request that reaches the data layer gets 5xx. */
const stub = new Proxy(
  {},
  { get: () => () => Effect.fail(new Error("The stub database has no rows.")) },
) as unknown as (typeof Database)["Service"];
const dbLayer = Layer.succeed(Database, stub);
const shared = {
  dbLayer,
  github: null,
  resources: Layer.mergeAll(
    dbLayer,
    Layer.succeed(UpdateHub, {} as (typeof UpdateHub)["Service"]),
    Layer.succeed(Storage, {} as (typeof Storage)["Service"]),
    Layer.succeed(GithubIntegration, {} as (typeof GithubIntegration)["Service"]),
    Layer.succeed(GithubPollState, githubPollState({ app: null, organisationId: null } as never)),
  ),
} as unknown as Resources;
const directory = Layer.succeed(
  AgentDirectory,
  AgentDirectory.of({
    find: (reference) => Effect.succeed(reference.toLowerCase() === "orion" ? [captain] : []),
  }),
);

const ircHookToken = "an-example-hook-token-of-at-least-32-bytes";
const mainPort = freePort();
const agentPort = freePort();
const options = config({
  DATABASE_URL: "postgresql://subspace@127.0.0.1/subspace",
  AUTH_PROVIDERS: "dev",
  ENABLE_DEV_AUTH: "true",
  NODE_ENV: "development",
  PORT: String(mainPort),
  AGENT_PORT: String(agentPort),
  IRC_CREDENTIAL_SECRET: "an example credential secret of at least 32 bytes",
  IRC_AUTH_HOOK_TOKEN: ircHookToken,
  IRC_AUTH_ORGANISATION: "dev:local",
});
let scope: Scope.Closeable;

beforeAll(async () => {
  scope = await Effect.runPromise(Scope.make());
  await Effect.runPromise(
    Layer.buildWithScope(
      server(options, crypto.getRandomValues(new Uint8Array(32)), {
        store: {} as (typeof Storage)["Service"],
        shared,
        directory,
      }),
      scope,
    ),
  );
});
afterAll(() => Effect.runPromise(Scope.close(scope, Exit.void)));

function call(
  port: number,
  path: string,
  method = "GET",
  headers: Record<string, string> = {},
  body?: unknown,
) {
  return fetch(`http://127.0.0.1:${port}${path}`, {
    method,
    headers: { ...(body === undefined ? {} : { "Content-Type": "application/json" }), ...headers },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(5000),
  });
}
const agent = { "X-Subspace-Agent": "Orion" };

describe("the API process with both listeners", () => {
  test("the agent port serves no sign-in, documentation, webhook or administrator route", async () => {
    for (const headers of [{}, agent]) {
      expect((await call(agentPort, "/api/v1/auth/dev/token", "POST", headers, {})).status).toBe(
        404,
      );
      expect((await call(agentPort, "/openapi.json", "GET", headers)).status).toBe(404);
      expect(
        (await call(agentPort, "/api/v1/integrations/github/webhook", "POST", headers, {})).status,
      ).toBe(404);
      for (const route of adminRoutes(agentApi.groups as never)) {
        const path = route.path.replace(/:[A-Za-z]+/gu, "00000000-0000-4000-8000-000000000099");
        const response = await call(
          agentPort,
          path,
          route.method,
          headers,
          route.method === "GET" ? undefined : {},
        );
        expect([route.method, path, response.status]).toEqual([route.method, path, 404]);
      }
    }
  });

  test("the agent port serves neither IRC credentials nor the IRC auth hook", async () => {
    const choices: Record<string, string>[] = [
      {},
      agent,
      { Authorization: `Bearer ${ircHookToken}` },
    ];
    for (const headers of choices) {
      for (const path of ["/api/v1/me/irc-credential", "/api/v1/irc/authenticate"])
        expect([path, (await call(agentPort, path, "POST", headers, {})).status]).toEqual([
          path,
          404,
        ]);
    }
  });

  test("the main port serves both IRC routes, the hook only with its token", async () => {
    // Routed, then refused for want of a signed-in user.
    expect((await call(mainPort, "/api/v1/me/irc-credential", "POST")).status).toBe(401);
    const hook = (headers: Record<string, string>) =>
      call(mainPort, "/api/v1/irc/authenticate", "POST", headers, { accountName: "orion" });
    expect((await hook({})).status).toBe(401);
    expect((await hook({ Authorization: "Bearer wrong-token" })).status).toBe(401);
    // The agent header grants nothing here either.
    expect((await hook(agent)).status).toBe(401);
    const accepted = await hook({ Authorization: `Bearer ${ircHookToken}` });
    expect(accepted.status).toBe(200);
    expect(await accepted.json()).toEqual({ success: false });
  });

  test("the agent port refuses a request without an agent and serves one with it", async () => {
    expect((await call(agentPort, "/api/v1/me")).status).toBe(401);
    const me = await call(agentPort, "/api/v1/me", "GET", agent);
    expect(me.status).toBe(200);
    expect(await me.json()).toMatchObject({ userId: captain.id, kind: "agent", isAdmin: false });
    expect(
      (await call(agentPort, "/api/v1/me", "GET", { ...agent, Origin: "http://localhost:3000" }))
        .status,
    ).toBe(403);
  });

  test("the main port keeps development sign-in, browser origins and the administrator routes", async () => {
    // Routed and validated, then refused only for its invalid body: the route is there.
    const invalid = await call(
      mainPort,
      "/api/v1/auth/dev/token",
      "POST",
      { Origin: "http://localhost:5173" },
      { email: 1 },
    );
    expect(invalid.status).toBe(400);
    // A valid request reaches the (stub) database: sign-in is served on this port.
    const valid = await call(
      mainPort,
      "/api/v1/auth/dev/token",
      "POST",
      { Origin: "http://127.0.0.1:5173" },
      { email: "dev@example.com" },
    );
    expect(valid.status).toBe(500);
    expect((await call(mainPort, "/openapi.json")).status).toBe(200);
    // The agent header alone grants nothing on the main port.
    expect((await call(mainPort, "/api/v1/me", "GET", agent)).status).toBe(401);
    for (const route of adminRoutes(agentApi.groups as never)) {
      const path = route.path.replace(/:[A-Za-z]+/gu, "00000000-0000-4000-8000-000000000099");
      const response = await call(
        mainPort,
        path,
        route.method,
        {},
        route.method === "GET" ? undefined : {},
      );
      expect([route.method, path, response.status]).toEqual([route.method, path, 401]);
    }
  });
});

test("the agent listener's route table is every agent route and no administrator route", async () => {
  const auth = Layer.succeed(BearerAuth, BearerAuth.of({ bearer: (next) => next as never }));
  const context = await Effect.runPromise(
    Effect.scoped(
      Layer.build(
        agentHandlerLayers().pipe(Layer.provide(Layer.merge(auth, validationErrors))) as never,
      ),
    ),
  );
  const services = (context as unknown as { mapUnsafe: Map<string, unknown> }).mapUnsafe;
  const table = Object.values(agentApi.groups).flatMap((group) =>
    (services.get(group.key) as { routes: { method: string; path: string }[] }).routes.map(
      (route) => `${route.method} ${route.path}`,
    ),
  );
  const admin = adminRoutes(agentApi.groups as never).map((r) => `${r.method} ${r.path}`);
  const expected = Object.values(agentApi.groups)
    .flatMap((group) =>
      Object.values(group.endpoints as Record<string, { method: string; path: string }>).map(
        (endpoint) => `${endpoint.method} ${endpoint.path}`,
      ),
    )
    .filter((route) => !admin.includes(route));
  expect(table.toSorted()).toEqual(expected.toSorted());
  for (const route of admin) expect(table).not.toContain(route);
  expect(admin).toContain("POST /api/v1/fleets");
  expect(table).toContain("GET /api/v1/fleets");
  expect(table).toContain("POST /mcp");
});
