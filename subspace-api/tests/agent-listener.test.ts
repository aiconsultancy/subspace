import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { drizzle } from "drizzle-orm/node-postgres";
import { BunHttpServer } from "@effect/platform-bun";
import { Effect, Layer } from "effect";
import { HttpRouter } from "effect/http";
import {
  adminOnlyEndpoints,
  agentLookup,
  adminOnlyTools,
  AgentDirectory,
  chooseAgent,
  adminRoutes,
} from "../src/agent-listener";
import { agentApi, agentRoutes, documentedApi } from "../src/api";
import { config } from "../src/config";
import { Database } from "../src/db";
import { GithubIntegration } from "../src/github";
import { GithubPollState, githubPollState } from "../src/github-poll";
import { type Member } from "../src/identity";
import { fleetAgentOf } from "../src/initiatives";
import { mcpInstructions, mcpTools } from "../src/mcp-tools";
import { Storage } from "../src/storage";
import { UpdateHub } from "../src/updates";

const org = "00000000-0000-4000-8000-000000000001";
const fleet = "00000000-0000-4000-8000-0000000000a1";
const created = new Date("2026-11-02T09:00:00.000Z");
const databaseUrl = "postgresql://subspace@127.0.0.1/subspace";

function person(id: string, fields: Partial<Member>): Member {
  return {
    id,
    organisationId: org,
    subject: `${id}@example.com`,
    email: null,
    displayName: "Example",
    role: "member",
    tokenRole: null,
    active: true,
    agentRank: null,
    agentFleet: null,
    agentFleetId: null,
    agentShipId: null,
    agentShip: null,
    agentModel: null,
    agentIrcNick: null,
    registeredById: null,
    deactivatedById: null,
    deactivatedAt: null,
    version: 1,
    createdAt: created,
    updatedAt: created,
    ...fields,
    // An agent's username is its IRC nick, as the database keeps it.
    username: fields.agentRank ? (fields.agentIrcNick ?? null) : (fields.username ?? null),
  };
}

const agentFields = (rank: Member["agentRank"]) => ({
  agentRank: rank,
  agentFleet: "alpha",
  agentFleetId: fleet,
  agentModel: "example-model",
  registeredById: "00000000-0000-4000-8000-0000000000ff",
});
const captain = person("00000000-0000-4000-8000-000000000011", {
  ...agentFields("captain"),
  displayName: "Orion",
  agentIrcNick: "orion-irc",
  agentShip: "Example ship",
  subject: "orion@agents.example.com",
});
const crewAgent = person("00000000-0000-4000-8000-000000000012", {
  ...agentFields("crew"),
  displayName: "Deckhand",
});
const retired = person("00000000-0000-4000-8000-000000000013", {
  ...agentFields("captain"),
  displayName: "Retired",
  active: false,
});
const twinByName = person("00000000-0000-4000-8000-000000000014", {
  ...agentFields("captain"),
  displayName: "Twin",
});
const twinByNick = person("00000000-0000-4000-8000-000000000015", {
  ...agentFields("rearAdmiral"),
  displayName: "Admiral",
  agentIrcNick: "twin",
});
// A human administrator: no header may ever resolve to this record.
const operator = person("00000000-0000-4000-8000-000000000016", {
  displayName: "Operator",
  role: "admin",
  subject: "operator@example.com",
  email: "operator@example.com",
});
const everyone = [captain, crewAgent, retired, twinByName, twinByNick, operator];

/** Matches as the SQL lookup does, but also offers human rows so the refusal of them is tested. */
const directory = Layer.succeed(
  AgentDirectory,
  AgentDirectory.of({
    find: (reference) =>
      Effect.succeed(
        everyone.filter(
          (row) =>
            row.displayName.toLowerCase() === reference.toLowerCase() ||
            row.agentIrcNick?.toLowerCase() === reference.toLowerCase() ||
            row.subject === reference,
        ),
      ),
  }),
);

/** Any database use answers with this failure, so a test can see a request reached the data layer. */
const reachedDatabase = new Error("The stub database was reached.");
const stub = new Proxy(
  {},
  {
    get: (_target, name) =>
      name === "transaction"
        ? (run: (tx: unknown) => unknown) =>
            Effect.suspend(
              () =>
                run(
                  new Proxy({}, { get: () => () => Effect.die(reachedDatabase) }),
                ) as Effect.Effect<unknown>,
            )
        : () => Effect.die(reachedDatabase),
  },
) as (typeof Database)["Service"];

const listener = config({
  DATABASE_URL: databaseUrl,
  AUTH_PROVIDERS: "dev",
  ENABLE_DEV_AUTH: "true",
  AGENT_PORT: "3001",
}).agent!;
const dbLayer = Layer.succeed(Database, stub);
const resources = Layer.mergeAll(
  dbLayer,
  Layer.succeed(UpdateHub, {} as (typeof UpdateHub)["Service"]),
  Layer.succeed(Storage, {} as (typeof Storage)["Service"]),
  Layer.succeed(GithubIntegration, {} as (typeof GithubIntegration)["Service"]),
  Layer.succeed(GithubPollState, githubPollState({ app: null, organisationId: null } as never)),
);
const app = agentRoutes(listener, { dbLayer, resources } as never, directory).pipe(
  Layer.provide(BunHttpServer.layerHttpServices),
);
const { handler, dispose } = HttpRouter.toWebHandler(app, { disableLogger: true });
afterAll(() => dispose());

function send(
  path: string,
  agent?: string,
  method = "GET",
  body?: unknown,
  extra: Record<string, string> = {},
) {
  const headers = new Headers({ Host: "127.0.0.1:3001", ...extra });
  if (agent !== undefined) headers.set("X-Subspace-Agent", agent);
  if (body !== undefined) headers.set("Content-Type", "application/json");
  return handler(
    new Request(`http://127.0.0.1:3001${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    }),
  );
}

async function refused(response: Response, status: number, detail?: string) {
  expect(response.status).toBe(status);
  expect(response.headers.get("content-type")).toContain("application/problem+json");
  const body = (await response.json()) as { status: number; detail: string };
  expect(body.status).toBe(status);
  if (detail) expect(body.detail).toContain(detail);
}

const rpc = (agent: string | undefined, method: string, params?: object) =>
  send("/mcp", agent, "POST", { jsonrpc: "2.0", id: 1, method, params });

describe("the agent listener's configuration", () => {
  const env = { DATABASE_URL: databaseUrl, AUTH_PROVIDERS: "dev", ENABLE_DEV_AUTH: "true" };

  test("is off unless a port is configured", () => {
    expect(config(env).agent).toBeNull();
  });

  test("defaults to the development organisation and the main listener's address", () => {
    expect(listener).toEqual({
      hostname: "127.0.0.1",
      port: 3001,
      provider: "dev",
      externalOrganisation: "local",
    });
    expect(
      config({ ...env, AGENT_PORT: "3001", AGENT_ORGANISATION: "dev:fleet-example" }).agent,
    ).toMatchObject({ externalOrganisation: "fleet-example" });
  });

  test("keeps to loopback, outside production, on its own port", () => {
    expect(() => config({ ...env, AGENT_PORT: "3000" })).toThrow("differ from PORT");
    expect(() => config({ ...env, AGENT_PORT: "0" })).toThrow("valid TCP port");
    expect(() => config({ ...env, AGENT_PORT: "3001", AGENT_HOST: "0.0.0.0" })).toThrow("loopback");
    expect(
      config({ ...env, AGENT_PORT: "3001", HOST: "0.0.0.0", DEV_CONTAINER_LOCAL_ONLY: "true" })
        .agent?.hostname,
    ).toBe("0.0.0.0");
    expect(() =>
      config({
        DATABASE_URL: databaseUrl,
        WORKOS_CLIENT_ID: "client_example",
        NODE_ENV: "production",
        AGENT_PORT: "3001",
        AGENT_ORGANISATION: "workos:org_example",
      }),
    ).toThrow("outside production");
    expect(() =>
      config({ DATABASE_URL: databaseUrl, WORKOS_CLIENT_ID: "client_example", AGENT_PORT: "3001" }),
    ).toThrow("AGENT_ORGANISATION");
    expect(() => config({ ...env, AGENT_PORT: "3001", AGENT_ORGANISATION: "local" })).toThrow(
      "AGENT_ORGANISATION",
    );
    // The listener answers only for an organisation of an enabled sign-in provider.
    expect(() =>
      config({ ...env, AGENT_PORT: "3001", AGENT_ORGANISATION: "workos:org_example" }),
    ).toThrow("AUTH_PROVIDERS");
    expect(() =>
      config({
        DATABASE_URL: databaseUrl,
        WORKOS_CLIENT_ID: "client_example",
        AGENT_PORT: "3001",
        AGENT_ORGANISATION: "dev:local",
      }),
    ).toThrow("AUTH_PROVIDERS");
    expect(
      config({
        DATABASE_URL: databaseUrl,
        WORKOS_CLIENT_ID: "client_example",
        AGENT_PORT: "3001",
        AGENT_ORGANISATION: "workos:org_example",
      }).agent,
    ).toMatchObject({ provider: "workos", externalOrganisation: "org_example" });
  });
});

describe("resolving the agent a header names", () => {
  const choose = (candidates: Member[], reference?: string) =>
    Effect.runPromise(Effect.result(chooseAgent(candidates, reference)));

  test("resolves one active agent record", async () => {
    expect(await choose([captain], "Orion")).toMatchObject({ success: captain });
  });

  test("refuses a missing header, an unknown name, a human, an inactive or an ambiguous agent", async () => {
    expect(await choose([], undefined)).toMatchObject({ failure: { status: 401 } });
    expect(await choose([], "  ")).toMatchObject({ failure: { status: 401 } });
    expect(await choose([], "Nobody")).toMatchObject({ failure: { status: 401 } });
    expect(await choose([operator], "Operator")).toMatchObject({ failure: { status: 401 } });
    expect(await choose([retired], "Retired")).toMatchObject({ failure: { status: 403 } });
    expect(await choose([twinByName, twinByNick], "Twin")).toMatchObject({
      failure: { status: 401 },
    });
    // A deactivated record sharing the name or nick does not lock out the active agent.
    expect(await choose([{ ...twinByName, active: false }, twinByNick], "Twin")).toMatchObject({
      success: twinByNick,
    });
    expect(
      await choose(
        [
          { ...twinByName, active: false },
          { ...twinByNick, active: false },
        ],
        "Twin",
      ),
    ).toMatchObject({ failure: { status: 403 } });
  });
});

test("the agent lookup puts active records first, before its limit", () => {
  const db = drizzle.mock();
  const { sql: text, params } = agentLookup(db as never, listener, "Orion").toSQL();
  expect(text).toMatch(/order by "users"\."active" desc, "users"\."id" asc limit \$\d+$/u);
  expect(params).toContain("orion");
  // Names compare in ASCII, as usernames and Ergo do, whatever the database locale.
  expect(text).toContain('lower("users"."agent_irc_nick" COLLATE "C")');
});

test("one active agent resolves however many deactivated records share its name or nick", async () => {
  const retiredCopies = [1, 2, 3, 4, 5].map((n) => ({
    ...captain,
    id: `00000000-0000-4000-8000-00000000010${n}`,
    active: false,
  }));
  for (const order of [
    [...retiredCopies, captain],
    [captain, ...retiredCopies],
  ])
    expect(await Effect.runPromise(Effect.result(chooseAgent(order, "Orion")))).toMatchObject({
      success: captain,
    });
});

describe("requests on the agent listener", () => {
  test("act as the named agent, by name or IRC nick, never as an administrator", async () => {
    for (const reference of ["Orion", "orion", "ORION-IRC"]) {
      const response = await send("/api/v1/me", reference);
      expect(response.status).toBe(200);
      expect(response.headers.get("cache-control")).toBe("private, no-store");
      expect(await response.json()).toEqual({
        userId: captain.id,
        kind: "agent",
        displayName: "Orion",
        email: null,
        username: "orion-irc",
        agentName: "Orion",
        agentRank: "captain",
        fleet: "alpha",
        ship: "Example ship",
        fleetId: fleet,
        shipId: null,
        isAdmin: false,
      });
    }
  });

  test("are refused without a valid agent", async () => {
    await refused(await send("/api/v1/me"), 401, "X-Subspace-Agent");
    await refused(await send("/api/v1/me", ""), 401);
    await refused(await send("/api/v1/me", "Nobody"), 401);
    await refused(await send("/api/v1/me", "Operator"), 401);
    await refused(await send("/api/v1/me", "operator@example.com"), 401);
    await refused(await send("/api/v1/me", "orion@agents.example.com"), 401);
    await refused(await send("/api/v1/me", "Retired"), 403, "deactivated");
    await refused(await send("/api/v1/me", "Twin"), 401, "more than one agent");
    await refused(await send("/api/v1/fleets"), 401);
    const mcp = await rpc(undefined, "tools/list");
    await refused(mcp, 401);
  });

  test("ignore a bearer token", async () => {
    const response = await handler(
      new Request("http://127.0.0.1:3001/api/v1/me", {
        headers: { Authorization: "Bearer example", Host: "127.0.0.1:3001" },
      }),
    );
    await refused(response, 401, "X-Subspace-Agent");
  });

  test("cannot reach development sign-in", async () => {
    for (const agent of [undefined, "Orion"]) {
      const response = await send("/api/v1/auth/dev/token", agent, "POST", {
        email: "operator@example.com",
      });
      expect(response.status).toBe(404);
    }
    expect((await send("/openapi.json", "Orion")).status).toBe(404);
  });

  test("cannot reach an administrator route, with or without an agent", async () => {
    const routes = adminRoutes(agentApi.groups as never);
    expect(routes.length).toBe(Object.values(adminOnlyEndpoints).flat().length);
    for (const route of routes) {
      const path = route.path.replace(/:[A-Za-z]+/gu, "00000000-0000-4000-8000-000000000099");
      for (const agent of [undefined, "Orion"]) {
        const response = await send(
          path,
          agent,
          route.method,
          route.method === "GET" ? undefined : {},
        );
        expect(response.status).toBe(404);
      }
      // However the path is spelt, the router has no such route on this listener.
      const encoded = path
        .replace("/api/v1/", "/api/v1/%")
        .replace(/%(.)/u, (_, letter: string) => `%${letter.charCodeAt(0).toString(16)}`);
      for (const spelling of [`/${path}`, path.replace("/api/v1/", "/api//v1//"), encoded]) {
        const response = await send(
          spelling,
          "Orion",
          route.method,
          route.method === "GET" ? undefined : {},
        );
        expect([spelling, response.status]).toEqual([spelling, 404]);
      }
    }
    // The same paths with other methods are member routes and still served.
    expect((await send("/api/v1/fleets", "Orion")).status).not.toBe(404);
  });

  test("refuse a browser Origin and a host that is not loopback", async () => {
    for (const path of ["/api/v1/me", "/mcp"]) {
      const method = path === "/mcp" ? "POST" : "GET";
      const body = path === "/mcp" ? { jsonrpc: "2.0", id: 1, method: "tools/list" } : undefined;
      await refused(
        await send(path, "Orion", method, body, { Origin: "http://127.0.0.1:3001" }),
        403,
        "Origin",
      );
      for (const host of ["attacker.example", "attacker.example:3001", "127.0.0.1.example", ""])
        await refused(await send(path, "Orion", method, body, { Host: host }), 403, "127.0.0.1");
      for (const host of ["127.0.0.1", "127.0.0.1:3001", "localhost:3001", "[::1]:3001"]) {
        const response = await send(path, "Orion", method, body, { Host: host });
        expect([host, response.status]).toEqual([host, 200]);
      }
    }
    // Refused before routing: an unknown path from a browser gets the same answer.
    await refused(await send("/nowhere", "Orion", "GET", undefined, { Origin: "null" }), 403);
  });

  test("lists and calls MCP tools as the agent, without administrator tools", async () => {
    const list = await rpc("Orion", "tools/list");
    expect(list.status).toBe(200);
    const names = (
      (await list.json()) as { result: { tools: { name: string }[] } }
    ).result.tools.map(({ name }) => name);
    expect(names).toContain("subspace_create_issue");
    for (const name of adminOnlyTools) {
      expect(Object.hasOwn(mcpTools, name)).toBe(true);
      expect(names).not.toContain(name);
      const call = await rpc("Orion", "tools/call", { name, arguments: {} });
      expect(((await call.json()) as { error: { code: number } }).error.code).toBe(-32602);
    }
    const whoami = await rpc("orion-irc", "tools/call", { name: "subspace_whoami", arguments: {} });
    expect(
      ((await whoami.json()) as { result: { structuredContent: unknown } }).result
        .structuredContent,
    ).toMatchObject({ userId: captain.id, kind: "agent", agentRank: "captain", isAdmin: false });
  });

  test("gives the rules for filing work in the MCP server instructions", async () => {
    const init = await rpc("Orion", "initialize", { protocolVersion: "2025-11-25" });
    expect(init.status).toBe(200);
    expect(
      ((await init.json()) as { result: { protocolVersion: string; instructions: string } }).result,
    ).toEqual(
      expect.objectContaining({ protocolVersion: "2025-11-25", instructions: mcpInstructions }),
    );
  });

  test("let a captain relay crew work and refuse it from any other rank", async () => {
    const crew = { name: "Example crew member", model: "example-model" };
    const body = { body: "Done.", relayedCrew: crew };
    // The captain's relay passes the crew check and reaches the data layer.
    const relayed = await send("/api/v1/issues/EXA-1/comments", "Orion", "POST", body);
    expect(relayed.status).toBe(500);
    await refused(
      await send("/api/v1/issues/EXA-1/comments", "Deckhand", "POST", body),
      400,
      "Only a captain or rear admiral may relay crew work.",
    );
    const tool = await rpc("Deckhand", "tools/call", {
      name: "subspace_create_comment",
      arguments: { issue: "EXA-1", ...body },
    });
    expect(
      (
        (await tool.json()) as {
          result: { isError: boolean; structuredContent: { detail: string } };
        }
      ).result,
    ).toMatchObject({
      isError: true,
      structuredContent: { detail: "Only a captain or rear admiral may relay crew work." },
    });
  });
});

test("the main API still documents development sign-in and the administrator routes", () => {
  const paths = Object.keys((documentedApi.groups as unknown as Record<string, unknown>) ?? {});
  expect(paths).toContain("development");
  expect(Object.keys(agentApi.groups)).not.toContain("development");
  expect(Object.keys(agentApi.groups)).not.toContain("githubWebhook");
});

// Against a running API with the agent listener on, both serving the same database: set
// TEST_API_URL, TEST_AGENT_API_URL and TEST_AGENT_ORGANISATION, the development organisation
// slug the agent listener's AGENT_ORGANISATION names (dev:<slug>).
const origin = process.env.TEST_API_URL?.replace(/\/$/u, "");
const agentOrigin = process.env.TEST_AGENT_API_URL?.replace(/\/$/u, "");
const agentOrganisation = process.env.TEST_AGENT_ORGANISATION;

describe.skipIf(!origin || !agentOrigin || !agentOrganisation)(
  "the agent listener over HTTP",
  () => {
    const suffix = crypto.randomUUID().slice(0, 8);
    const captainName = `Listener-captain-${suffix}`;
    let admin = "";
    let projectKey = "";
    let foreignKey = "";

    async function main<T>(status: number, path: string, method = "GET", data?: unknown) {
      const response = await fetch(`${origin}/api/v1${path}`, {
        method,
        headers: {
          ...(admin ? { Authorization: `Bearer ${admin}` } : {}),
          ...(data === undefined ? {} : { "Content-Type": "application/json" }),
        },
        body: data === undefined ? undefined : JSON.stringify(data),
        signal: AbortSignal.timeout(10_000),
      });
      expect(response.status).toBe(status);
      return (await response.json()) as T;
    }

    const agent = (path: string, name: string, method = "GET", data?: unknown) =>
      fetch(`${agentOrigin}${path}`, {
        method,
        headers: {
          "X-Subspace-Agent": name,
          ...(data === undefined ? {} : { "Content-Type": "application/json" }),
        },
        body: data === undefined ? undefined : JSON.stringify(data),
        signal: AbortSignal.timeout(10_000),
      });

    beforeAll(async () => {
      admin = (
        await main<{ accessToken: string }>(200, "/auth/dev/token", "POST", {
          email: `listener-admin-${suffix}@example.com`,
          organisation: agentOrganisation,
        })
      ).accessToken;
      const own = await main<{ id: string }>(201, "/fleets", "POST", {
        slug: `listener-${suffix}`,
        name: "Example listener fleet",
        ircHost: "irc.example.test",
        ircPort: 6697,
        ircTls: true,
      });
      const other = await main<{ id: string }>(201, "/fleets", "POST", {
        slug: `elsewhere-${suffix}`,
        name: "Example other fleet",
        ircHost: "irc.example.test",
        ircPort: 6697,
        ircTls: true,
      });
      await main(201, "/agents", "POST", {
        name: captainName,
        rank: "captain",
        fleetId: own.id,
        ship: "Example ship",
        defaultModel: "example-model",
        serviceTokenClientId: `listener-${suffix}@agents.example.com`,
      });
      const programme = async (name: string, fleetId: string) => {
        const created = await main<{ id: string }>(201, "/initiatives", "POST", {
          name: `${name} ${suffix}`,
        });
        await main(200, `/initiatives/${created.id}/fleet`, "PUT", { fleetId });
        return created.id;
      };
      const project = async (key: string, initiativeId: string) => {
        const created = await main<{ version: number }>(201, "/projects", "POST", {
          key,
          name: `Listener ${key}`,
        });
        await main(200, `/projects/${key}/initiative`, "PUT", {
          initiativeId,
          version: created.version,
        });
        return key;
      };
      const tag = suffix
        .replace(/[^a-z]/gu, "")
        .slice(0, 3)
        .toUpperCase()
        .padEnd(3, "X");
      projectKey = await project(`L${tag}`, await programme("Listener programme", own.id));
      foreignKey = await project(`F${tag}`, await programme("Elsewhere programme", other.id));
    });

    test("an agent reads and writes as itself", async () => {
      const me = await agent("/api/v1/me", captainName);
      expect(me.status).toBe(200);
      expect(await me.json()).toMatchObject({
        kind: "agent",
        agentRank: "captain",
        isAdmin: false,
      });
      const initiatives = (await (await agent("/api/v1/me/initiatives", captainName)).json()) as {
        items: { name: string }[];
      };
      expect(initiatives.items.map(({ name }) => name)).toEqual([`Listener programme ${suffix}`]);
      const created = await agent("/api/v1/issues", captainName, "POST", {
        projectId: (await main<{ id: string }>(200, `/projects/${projectKey}`)).id,
        title: "Written by an agent",
        summary: "Agent listener fixture",
        relayedCrew: { name: "Example crew member", model: "example-model" },
      });
      expect(created.status).toBe(201);
      const issue = (await created.json()) as { identifier: string; relayedCrew: unknown };
      expect(issue.relayedCrew).toEqual({ name: "Example crew member", model: "example-model" });
      const comment = await agent(
        `/api/v1/issues/${issue.identifier}/comments`,
        captainName,
        "POST",
        {
          body: "A comment from the agent listener.",
        },
      );
      expect(comment.status).toBe(201);
    });

    test("an agent keeps its fleet limits", async () => {
      const response = await agent("/api/v1/issues", captainName, "POST", {
        projectId: (await main<{ id: string }>(200, `/projects/${foreignKey}`)).id,
        title: "Dispatched elsewhere",
        summary: "Agent listener fixture",
        readyForAgents: true,
      });
      expect(response.status).toBe(403);
    });

    test("sign-in and administrator routes are absent, and humans are refused", async () => {
      expect((await agent("/api/v1/auth/dev/token", captainName, "POST", {})).status).toBe(404);
      expect((await agent("/api/v1/fleets", captainName, "POST", {})).status).toBe(404);
      expect((await agent("/api/v1/me", `listener-admin-${suffix}@example.com`)).status).toBe(401);
    });
  },
);

test("a fleet's agents count as members of the initiatives that belong to their fleet", () => {
  const row = (id: string, parentId: string | null, fleetId: string | null) => ({
    initiative: { id, parentId, fleetId } as never,
  });
  const other = "00000000-0000-4000-8000-0000000000b1";
  const rows = new Map(
    [
      row("root", null, fleet),
      row("child", "root", null),
      row("moved", "root", other),
      row("loose", null, null),
    ].map((entry) => [(entry.initiative as { id: string }).id, entry]),
  ) as never as Map<string, { initiative: Parameters<typeof fleetAgentOf>[1] }>;
  const of = (member: Member, id: string) =>
    fleetAgentOf(member, rows.get(id)!.initiative, rows as never);
  expect(["root", "child", "moved", "loose"].filter((id) => of(captain, id))).toEqual([
    "root",
    "child",
  ]);
  expect(of(operator, "root")).toBe(false);
  expect(of({ ...captain, agentFleetId: null }, "root")).toBe(false);
});
