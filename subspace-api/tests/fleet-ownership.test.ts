import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { BunHttpServer } from "@effect/platform-bun";
import { getTableName, type SQL } from "drizzle-orm";
import { PgDialect } from "drizzle-orm/pg-core";
import { Effect, Layer } from "effect";
import { HttpRouter } from "effect/http";
import { HttpApi, HttpApiBuilder } from "effect/http-api";
import pg from "pg";
import { Database } from "../src/db";
import {
  dispatchesIssue,
  foreignFleetDetail,
  foreignFleetMoveDetail,
  foreignFleetReparentDetail,
  lockInitiativeFleet,
  lockProjectFleet,
  requireFleetOwnership,
  requireMoveOwnership,
  requireReparentOwnership,
} from "../src/fleet-ownership";
import { CurrentMember, type Member } from "../src/identity";
import { assignFleet } from "../src/initiatives";
import { issueGroup, issuesHandlers } from "../src/issues-http";
import { BearerAuth, validationErrors } from "../src/middleware";
import { problem } from "../src/problems";
import { initiatives, projects, users } from "../src/schema";
import { localDatabase } from "./local-database";

type Db = (typeof Database)["Service"];

const org = "00000000-0000-4000-8000-000000000001";
const agentId = "00000000-0000-4000-8000-000000000002";
const projectId = "00000000-0000-4000-8000-000000000003";
const alpha = "00000000-0000-4000-8000-0000000000a1";
const beta = "00000000-0000-4000-8000-0000000000b1";
const [child, parent, root] = [
  "00000000-0000-4000-8000-0000000000c1",
  "00000000-0000-4000-8000-0000000000c2",
  "00000000-0000-4000-8000-0000000000c3",
];
const later = "00000000-0000-4000-8000-000000000009";
const forbidden = problem(403, foreignFleetDetail);
const forbiddenMove = problem(403, foreignFleetMoveDetail);
const forbiddenReparent = problem(403, foreignFleetReparentDetail);
const dialect = new PgDialect();

/**
 * A database that answers each locked read in order with the next scripted rows and records the
 * table and lock mode of every read, and the row ID it looked up. A read without a row lock, or
 * out of the scripted order, fails the test.
 */
function scripted(...replies: { table: unknown; rows: unknown[] }[]) {
  const reads: string[] = [];
  const keys: unknown[] = [];
  const db = {
    select: () => ({
      from: (table: Parameters<typeof getTableName>[0]) => ({
        where: (condition: SQL) => ({
          for: (lock: string) => {
            const next = replies.shift();
            if (next?.table !== table)
              throw new Error(`Unexpected read of ${getTableName(table)} for ${lock}.`);
            reads.push(`${getTableName(table)} for ${lock}`);
            keys.push(dialect.sqlToQuery(condition).params.at(-1));
            return Effect.succeed(next.rows);
          },
        }),
      }),
    }),
  };
  return { db: db as unknown as Db, reads, keys, remaining: replies };
}

const agent = (fleetId: string | null) => ({
  table: users,
  rows: [{ agentRank: "rearAdmiral", fleetId }],
});
const project = (initiativeId: string | null) => ({ table: projects, rows: [{ initiativeId }] });
const initiative = (parentId: string | null, fleetId: string | null) => ({
  table: initiatives,
  rows: [{ parentId, fleetId }],
});
const member = (agentRank: Member["agentRank"]) => ({
  id: agentId,
  organisationId: org,
  agentRank,
});
const assign = { assigneeId: "00000000-0000-4000-8000-000000000004" };

function check(
  db: Db,
  input: { assigneeId?: string | null; readyForAgents?: boolean } = assign,
  rank: Member["agentRank"] = "rearAdmiral",
) {
  return Effect.runSync(
    Effect.match(requireFleetOwnership(db, member(rank), projectId, input), {
      onFailure: (error) => error,
      onSuccess: () => "allowed" as const,
    }),
  );
}

test("a dispatch names a non-null assignee or carries readyForAgents with either value", () => {
  expect(dispatchesIssue({ assigneeId: agentId })).toBe(true);
  expect(dispatchesIssue({ readyForAgents: true })).toBe(true);
  expect(dispatchesIssue({ readyForAgents: false })).toBe(true);
  expect(dispatchesIssue({ assigneeId: null, readyForAgents: true })).toBe(true);
  expect(dispatchesIssue({ assigneeId: null })).toBe(false);
  expect(dispatchesIssue({})).toBe(false);
});

test("humans and agents' non-dispatch writes read nothing, and agents without a fleet stop at their registration", () => {
  const none = scripted();
  expect(check(none.db, assign, null)).toBe("allowed");
  expect(check(none.db, { readyForAgents: true }, null)).toBe("allowed");
  expect(check(none.db, { assigneeId: null })).toBe("allowed");
  expect(check(none.db, {})).toBe("allowed");
  expect(none.reads).toEqual([]);
  const unfleeted = scripted(agent(null));
  expect(check(unfleeted.db)).toBe("allowed");
  expect(unfleeted.reads).toEqual(["users for share"]);
});

test("an agent may dispatch in its own fleet's project, and every row on the path is share-locked", () => {
  const own = scripted(agent(alpha), project(child), initiative(null, alpha));
  expect(check(own.db, { readyForAgents: false })).toBe("allowed");
  expect(own.reads).toEqual(["users for share", "projects for share", "initiatives for share"]);
});

test("the nearest initiative with a fleet decides, and ancestors above it are not read", () => {
  const inherited = scripted(
    agent(alpha),
    project(child),
    initiative(parent, null),
    initiative(root, alpha),
  );
  expect(check(inherited.db)).toBe("allowed");
  expect(inherited.reads).toEqual([
    "users for share",
    "projects for share",
    "initiatives for share",
    "initiatives for share",
  ]);
  const overridden = scripted(
    agent(alpha),
    project(child),
    initiative(parent, beta),
    initiative(null, alpha),
  );
  expect(check(overridden.db)).toEqual(forbidden);
  expect(overridden.remaining).toHaveLength(1);
});

test("a project in another fleet is refused with the forbidden problem", () => {
  expect(check(scripted(agent(alpha), project(child), initiative(null, beta)).db)).toEqual(
    forbidden,
  );
  expect(
    check(
      scripted(agent(alpha), project(child), initiative(parent, null), initiative(null, beta)).db,
      { readyForAgents: true },
    ),
  ).toEqual(forbidden);
});

test("the agent's fleet is read again in the write, not taken from the request", () => {
  // The check takes no fleet from the request's member. The locked registration places it in beta.
  expect(check(scripted(agent(beta), project(child), initiative(null, alpha)).db)).toEqual(
    forbidden,
  );
});

test("a project without an initiative, or with no fleet on its path, is open to every agent", () => {
  expect(check(scripted(agent(alpha), project(null)).db)).toBe("allowed");
  expect(
    check(
      scripted(agent(alpha), project(child), initiative(parent, null), initiative(null, null)).db,
    ),
  ).toBe("allowed");
});

test("a cyclic or broken initiative path ends without a fleet", () => {
  const cyclic = scripted(project(child), initiative(parent, null), initiative(child, null));
  expect(Effect.runSync(lockProjectFleet(cyclic.db, org, projectId))).toBeNull();
  expect(cyclic.remaining).toEqual([]);
  const broken = scripted(project(child), { table: initiatives, rows: [] });
  expect(Effect.runSync(lockProjectFleet(broken.db, org, projectId))).toBeNull();
  const missing = scripted({ table: projects, rows: [] });
  expect(Effect.runSync(Effect.flip(lockProjectFleet(missing.db, org, projectId)))).toEqual(
    problem(404, "Project not found."),
  );
});

function move(db: Db, source: string, destination: string, rank: Member["agentRank"] = "captain") {
  return Effect.runSync(
    Effect.match(requireMoveOwnership(db, member(rank), source, destination), {
      onFailure: (error) => error,
      onSuccess: () => "allowed" as const,
    }),
  );
}

test("humans and agents without a fleet move freely", () => {
  const none = scripted();
  expect(move(none.db, later, projectId, null)).toBe("allowed");
  expect(none.reads).toEqual([]);
  const unfleeted = scripted(agent(null));
  expect(move(unfleeted.db, later, projectId)).toBe("allowed");
  expect(unfleeted.reads).toEqual(["users for share"]);
});

test("a move locks both ownership paths in project ID order and may join own and unfleeted projects", () => {
  const allowed = scripted(agent(alpha), project(null), project(child), initiative(null, alpha));
  expect(move(allowed.db, later, projectId)).toBe("allowed");
  expect(allowed.reads).toEqual([
    "users for share",
    "projects for share",
    "projects for share",
    "initiatives for share",
  ]);
  // The destination sorts first, so its path is locked before the source's.
  expect(allowed.keys).toEqual([agentId, projectId, later, child]);
  const reversed = scripted(agent(alpha), project(null), project(null));
  expect(move(reversed.db, projectId, later)).toBe("allowed");
  expect(reversed.keys).toEqual([agentId, projectId, later]);
});

test("a move out of or into another fleet's project is refused with the move problem", () => {
  const outOf = scripted(
    agent(alpha),
    project(child),
    initiative(null, alpha),
    project(parent),
    initiative(null, beta),
  );
  expect(move(outOf.db, later, projectId)).toEqual(forbiddenMove);
  expect(outOf.keys).toEqual([agentId, projectId, child, later, parent]);
  // The first path in project ID order is already foreign, so the second is not read.
  const into = scripted(agent(alpha), project(child), initiative(null, beta), project(parent));
  expect(move(into.db, later, projectId)).toEqual(forbiddenMove);
  expect(into.remaining).toHaveLength(1);
});

function reparent(
  db: Db,
  initiativeId: string,
  parentId: string | null,
  rank: Member["agentRank"] = "rearAdmiral",
) {
  return Effect.runSync(
    Effect.match(requireReparentOwnership(db, member(rank), initiativeId, parentId), {
      onFailure: (error) => error,
      onSuccess: () => "allowed" as const,
    }),
  );
}

test("an initiative's own fleet decides before its ancestors', and a null start has no fleet", () => {
  const own = scripted(initiative(parent, beta));
  expect(Effect.runSync(lockInitiativeFleet(own.db, org, child))).toBe(beta);
  expect(own.reads).toEqual(["initiatives for share"]);
  const none = scripted();
  expect(Effect.runSync(lockInitiativeFleet(none.db, org, null))).toBeNull();
  expect(none.reads).toEqual([]);
});

test("humans and agents without a fleet reparent initiatives freely", () => {
  const none = scripted();
  expect(reparent(none.db, child, parent, null)).toBe("allowed");
  expect(none.reads).toEqual([]);
  const unfleeted = scripted(agent(null));
  expect(reparent(unfleeted.db, child, null)).toBe("allowed");
  expect(unfleeted.reads).toEqual(["users for share"]);
});

test("a reparent locks both ownership paths in initiative ID order and may join own and unfleeted initiatives", () => {
  // The new parent sorts first, so its path is locked before the initiative's.
  const allowed = scripted(
    agent(alpha),
    initiative(root, null),
    initiative(null, alpha),
    initiative(null, null),
  );
  expect(reparent(allowed.db, parent, child)).toBe("allowed");
  expect(allowed.reads).toEqual([
    "users for share",
    "initiatives for share",
    "initiatives for share",
    "initiatives for share",
  ]);
  expect(allowed.keys).toEqual([agentId, child, root, parent]);
  // A null parent makes the initiative a root, so only the initiative's own path is read.
  const detached = scripted(agent(alpha), initiative(parent, null), initiative(null, alpha));
  expect(reparent(detached.db, child, null)).toBe("allowed");
  expect(detached.keys).toEqual([agentId, child, parent]);
});

test("reparenting another fleet's initiative out of it, or any initiative into it, is refused", () => {
  const inherited = scripted(agent(alpha), initiative(parent, null), initiative(null, beta));
  expect(reparent(inherited.db, child, null)).toEqual(forbiddenReparent);
  // The initiative's own fleet decides, so ancestors above it are not read.
  const own = scripted(agent(alpha), initiative(parent, beta), initiative(null, alpha));
  expect(reparent(own.db, child, null)).toEqual(forbiddenReparent);
  expect(own.remaining).toHaveLength(1);
  // The new parent's path is foreign, so the initiative's path is not read.
  const into = scripted(agent(alpha), initiative(null, beta), initiative(null, null));
  expect(reparent(into.db, parent, child)).toEqual(forbiddenReparent);
  expect(into.remaining).toHaveLength(1);
  const intoLater = scripted(agent(alpha), initiative(null, alpha), initiative(root, beta));
  expect(reparent(intoLater.db, child, parent)).toEqual(forbiddenReparent);
  expect(intoLater.keys).toEqual([agentId, child, parent]);
});

// Serves the real issue routes with a database whose transactions fail as a refused dispatch
// would, so the wire shape of the 403 is checked without PostgreSQL.
describe("the refusal on the issue routes", () => {
  const refusing = new Proxy(
    {},
    {
      get: (_target, name) =>
        name === "transaction"
          ? () => Effect.fail(forbidden)
          : () => {
              throw new Error("The stub database has no rows.");
            },
    },
  );
  const created = new Date("2026-11-02T09:00:00.000Z");
  const rearAdmiral: Member = {
    id: agentId,
    organisationId: org,
    subject: "admiral@example.com",
    email: "admiral@example.com",
    displayName: "Example-admiral",
    role: "member",
    tokenRole: null,
    active: true,
    agentRank: "rearAdmiral",
    agentFleet: "alpha",
    agentFleetId: alpha,
    agentShipId: null,
    agentShip: null,
    agentModel: "example-model",
    agentIrcNick: null,
    username: null,
    registeredById: null,
    deactivatedById: null,
    deactivatedAt: null,
    version: 1,
    createdAt: created,
    updatedAt: created,
  };
  const auth = Layer.succeed(
    BearerAuth,
    BearerAuth.of({
      bearer: (next) =>
        Effect.provideService(next, CurrentMember, {
          ...rearAdmiral,
          tokenExpiresAt: 0,
          roleSource: "provisioned",
          tokenGrant: null,
        }),
    }),
  );
  const api = HttpApi.make("fleet-ownership-test").add(issueGroup);
  const app = HttpApiBuilder.layer(api).pipe(
    Layer.provide(HttpApiBuilder.group(api, "issues", issuesHandlers)),
    Layer.provide(Layer.mergeAll(auth, validationErrors)),
    HttpRouter.provideRequest(Layer.succeed(Database, refusing as Db)),
    Layer.provide(BunHttpServer.layerHttpServices),
  );
  const { handler, dispose } = HttpRouter.toWebHandler(app, { disableLogger: true });
  afterAll(() => dispose());

  test("an agent cannot set or clear an initiative's fleet, before any read", async () => {
    // Agents are always members, never admins, so the admin check refuses them first.
    for (const fleetId of [beta, null])
      expect(
        await Effect.runPromise(
          Effect.flip(assignFleet(rearAdmiral, child, fleetId)).pipe(
            Effect.provideService(Database, refusing as Db),
          ),
        ),
      ).toEqual(problem(403, "Fleet assignment requires organisation admin access."));
  });

  test.each([
    ["POST", "/issues", { projectId, title: "Example", summary: "Example", ...assign }],
    ["PATCH", "/issues/EXA-1", { version: 1, readyForAgents: true }],
  ] as const)("%s %s returns the problem document with status 403", async (method, path, body) => {
    const response = await handler(
      new Request(`http://127.0.0.1/api/v1${path}`, {
        method,
        headers: { Authorization: "Bearer example", "Content-Type": "application/json" },
        body: JSON.stringify(body),
      }),
    );
    expect(response.status).toBe(403);
    expect(response.headers.get("content-type")).toContain("application/problem+json");
    expect(await response.json()).toEqual({
      type: "about:blank",
      title: "Forbidden",
      status: 403,
      detail: foreignFleetDetail,
      currentVersion: null,
      errors: {},
    });
  });
});

const origin = process.env.TEST_API_URL?.replace(/\/$/u, "");
const testDatabase = process.env.TEST_DATABASE_URL;

const suffix = crypto.randomUUID().slice(0, 8);

type Role = "admin" | "member" | "admiral" | "drifter";
interface Issue {
  id: string;
  identifier: string;
  projectId: string;
  version: number;
  readyForAgents: boolean;
  assignee: { userId: string } | null;
}
interface Versioned {
  id: string;
  version: number;
}

describe.skipIf(!origin)("fleet ownership of dispatch writes over HTTP", () => {
  const tokens = new Map<Role, string>();
  const ids = new Map<Role, string>();
  const fleets = { alpha: "", beta: "" };
  const programmes = new Map<string, string>();
  const keys = new Map<string, string>();
  let captainId: string;

  async function http(path: string, role?: Role, method = "GET", data?: unknown) {
    const headers = new Headers();
    const token = role ? tokens.get(role) : undefined;
    if (token) headers.set("Authorization", `Bearer ${token}`);
    if (data !== undefined) headers.set("Content-Type", "application/json");
    if (path === "/mcp") {
      headers.set("Accept", "application/json, text/event-stream");
      headers.set("MCP-Protocol-Version", "2025-11-25");
    }
    return fetch(`${origin}${path === "/mcp" ? "" : "/api/v1"}${path}`, {
      method,
      headers,
      body: data === undefined ? undefined : JSON.stringify(data),
      signal: AbortSignal.timeout(10_000),
    });
  }

  async function call<T>(
    expected: number,
    path: string,
    role: Role,
    method = "GET",
    data?: unknown,
  ) {
    const response = await http(path, role, method, data);
    if (response.status !== expected) {
      const body = (await response.json().catch(() => ({}))) as { detail?: string };
      throw new Error(
        `${method} ${path}: ${response.status}, expected ${expected}. ${body.detail}`,
      );
    }
    return (await response.json()) as T;
  }

  async function signIn(role: Role, email: string) {
    const response = await http("/auth/dev/token", undefined, "POST", {
      email,
      organisation: `fleet-ownership-${suffix}`,
    });
    expect(response.status).toBe(200);
    tokens.set(role, ((await response.json()) as { accessToken: string }).accessToken);
    ids.set(role, (await call<{ userId: string }>(200, "/me", role)).userId);
  }

  async function tool<T>(name: string, args: object) {
    const response = await http("/mcp", "admiral", "POST", {
      jsonrpc: "2.0",
      id: 1,
      method: "tools/call",
      params: { name, arguments: args },
    });
    expect(response.status).toBe(200);
    return ((await response.json()) as { result: { structuredContent: T; isError: boolean } })
      .result;
  }

  async function programme(
    name: string,
    fleetId: string | null,
    parentId?: string,
    owner: Role = "admin",
  ) {
    const created = await call<Versioned>(201, "/initiatives", "admin", "POST", {
      name: `${name} ${suffix}`,
      ownerId: ids.get(owner),
      ...(parentId ? { parentId } : {}),
    });
    if (fleetId) await call(200, `/initiatives/${created.id}/fleet`, "admin", "PUT", { fleetId });
    programmes.set(name, created.id);
    return created.id;
  }

  async function project(key: string, initiativeId: string | null) {
    const created = await call<Versioned>(201, "/projects", "admin", "POST", {
      key,
      name: `Ownership ${key}`,
    });
    if (initiativeId)
      await call(200, `/projects/${key}/initiative`, "admin", "PUT", {
        initiativeId,
        version: created.version,
      });
    keys.set(key, created.id);
  }

  const issue = (key: string, title: string, fields: object = {}) =>
    call<Issue>(201, "/issues", "admin", "POST", {
      projectId: keys.get(key),
      title,
      summary: "Ownership fixture",
      ...fields,
    });
  const read = (id: string) => call<Issue>(200, `/issues/${id}`, "admin");
  const activity = async (id: string) =>
    (await call<{ items: { id: string }[] }>(200, `/issues/${id}/activity`, "admin")).items.map(
      ({ id: event }) => event,
    );
  const edit = (role: Role, target: Issue, changes: object) =>
    http(`/issues/${target.id}`, role, "PATCH", { version: target.version, ...changes });

  async function refused(response: Response, detail = foreignFleetDetail) {
    expect(response.status).toBe(403);
    expect(response.headers.get("content-type")).toContain("application/problem+json");
    expect(await response.json()).toEqual({
      type: "about:blank",
      title: "Forbidden",
      status: 403,
      detail,
      currentVersion: null,
      errors: {},
    });
  }

  beforeAll(async () => {
    await signIn("admin", "admin@example.com");
    await signIn("member", "member@example.com");
    for (const name of ["alpha", "beta"] as const) {
      fleets[name] = (
        await call<Versioned>(201, "/fleets", "admin", "POST", {
          slug: `${name}-${suffix}`,
          name: `Example ${name} fleet`,
          ircHost: "irc.example.test",
          ircPort: 6697,
          ircTls: true,
        })
      ).id;
    }
    const register = (name: string, rank: string, fields: object) =>
      call<{ userId: string }>(201, "/agents", "admin", "POST", {
        name,
        rank,
        defaultModel: "example-model",
        serviceTokenClientId: `${name.toLowerCase()}@example.com`,
        ...fields,
      });
    await register("Alpha-admiral", "rearAdmiral", { fleetId: fleets.alpha });
    captainId = (
      await register("Alpha-captain", "captain", { fleetId: fleets.alpha, ship: "Example ship" })
    ).userId;
    // An unknown fleet slug leaves the agent without a fleet.
    await register("Drifter", "rearAdmiral", { fleet: `unlisted-${suffix}` });
    await signIn("admiral", "alpha-admiral@example.com");
    await signIn("drifter", "drifter@example.com");
    expect(await call(200, "/me", "admiral")).toMatchObject({
      kind: "agent",
      fleetId: fleets.alpha,
    });
    expect(await call(200, "/me", "drifter")).toMatchObject({ kind: "agent", fleetId: null });

    const alphaProgramme = await programme("Alpha programme", fleets.alpha);
    await project("OWN", alphaProgramme);
    await project("KIN", await programme("Alpha workstream", null, alphaProgramme));
    await project("FOE", await programme("Beta programme", fleets.beta));
    await project("FREE", await programme("Unfleeted programme", null));
    await project("LONE", null);
    await project("MOVE", alphaProgramme);
    await project("SHIFT", await programme("Shifting programme", fleets.alpha));
    await project("RACE", alphaProgramme);
    await project("TIDE", await programme("Tidal programme", fleets.alpha));
  });

  test.each(["OWN", "KIN"])(
    "an agent dispatches and creates dispatched issues in %s, which its fleet owns",
    async (key) => {
      const offered = await issue(key, "Offer fixture");
      const flagged = await edit("admiral", offered, { readyForAgents: true });
      expect(flagged.status).toBe(200);
      const ready = (await flagged.json()) as Issue;
      expect(ready.readyForAgents).toBe(true);
      const taken = await edit("admiral", ready, { assigneeId: captainId });
      expect(taken.status).toBe(200);
      expect(await taken.json()).toMatchObject({
        assignee: { userId: captainId },
        readyForAgents: false,
      });
      await call(201, "/issues", "admiral", "POST", {
        projectId: keys.get(key),
        title: "Agent dispatched fixture",
        summary: "Ownership fixture",
        assigneeId: captainId,
      });
    },
  );

  test("another fleet's issue is refused over REST and MCP, and nothing changes", async () => {
    const target = await issue("FOE", "Foreign fixture", { readyForAgents: true });
    const before = { issue: await read(target.id), activity: await activity(target.id) };
    await refused(await edit("admiral", target, { assigneeId: captainId }));
    await refused(await edit("admiral", target, { readyForAgents: false }));
    await refused(
      await edit("admiral", target, { assigneeId: null, readyForAgents: true, title: "Retitled" }),
    );
    const mcp = await tool<{ status: number; detail: string }>("subspace_update_issue", {
      issue: target.identifier,
      changes: { version: target.version, readyForAgents: false },
    });
    expect(mcp.isError).toBe(true);
    expect(mcp.structuredContent).toMatchObject({ status: 403, detail: foreignFleetDetail });
    expect(await read(target.id)).toEqual(before.issue);
    expect(await activity(target.id)).toEqual(before.activity);
    expect(before.issue).toMatchObject({ readyForAgents: true, assignee: null });
  });

  test("clearing an assignee and other edits on another fleet's issue stay open", async () => {
    const target = await issue("FOE", "Foreign assigned fixture", { assigneeId: captainId });
    const retitled = await edit("admiral", target, { title: "Retitled foreign fixture" });
    expect(retitled.status).toBe(200);
    const cleared = await edit("admiral", (await retitled.json()) as Issue, { assigneeId: null });
    expect(cleared.status).toBe(200);
    expect(await cleared.json()).toMatchObject({ assignee: null, readyForAgents: false });
  });

  test("a project that moves to another fleet after the agent read the issue is refused", async () => {
    const moved = await issue("MOVE", "Moved project fixture");
    const seen = await call<Issue>(200, `/issues/${moved.id}`, "admiral");
    const current = await call<Versioned>(200, "/projects/MOVE", "admin");
    await call(200, "/projects/MOVE/initiative", "admin", "PUT", {
      initiativeId: programmes.get("Beta programme"),
      version: current.version,
    });
    expect((await read(moved.id)).version).toBe(seen.version);
    await refused(await edit("admiral", seen, { assigneeId: captainId }));

    const shifted = await issue("SHIFT", "Shifted initiative fixture");
    const seenShift = await call<Issue>(200, `/issues/${shifted.id}`, "admiral");
    await call(200, `/initiatives/${programmes.get("Shifting programme")}/fleet`, "admin", "PUT", {
      fleetId: fleets.beta,
    });
    expect((await read(shifted.id)).version).toBe(seenShift.version);
    await refused(await edit("admiral", seenShift, { readyForAgents: true }));
  });

  test.each(["FREE", "LONE"])(
    "projects without a fleet stay open to fleet agents (%s)",
    async (key) => {
      const target = await issue(key, "Unfleeted fixture");
      const flagged = await edit("admiral", target, { readyForAgents: true });
      expect(flagged.status).toBe(200);
      const assigned = await edit("admiral", (await flagged.json()) as Issue, {
        assigneeId: captainId,
      });
      expect(assigned.status).toBe(200);
    },
  );

  test.each(["member", "drifter"] as const)(
    "%s may dispatch another fleet's issue",
    async (role) => {
      const target = await issue("FOE", `Unrestricted ${role} fixture`);
      const flagged = await edit(role, target, { readyForAgents: true });
      expect(flagged.status).toBe(200);
      const assigned = await edit(role, (await flagged.json()) as Issue, {
        assigneeId: captainId,
      });
      expect(assigned.status).toBe(200);
    },
  );

  test("creating a dispatched issue in another fleet's project is refused", async () => {
    const count = async () =>
      (await call<{ items: unknown[] }>(200, "/issues?project=FOE&limit=200", "admin")).items
        .length;
    const before = await count();
    const fields = { projectId: keys.get("FOE"), title: "Refused", summary: "Ownership fixture" };
    await refused(await http("/issues", "admiral", "POST", { ...fields, assigneeId: captainId }));
    await refused(await http("/issues", "admiral", "POST", { ...fields, readyForAgents: true }));
    await refused(await http("/issues", "admiral", "POST", { ...fields, readyForAgents: false }));
    const mcp = await tool<{ status: number; detail: string }>("subspace_create_issue", {
      project: "FOE",
      title: "Refused over MCP",
      summary: "Ownership fixture",
      assigneeId: captainId,
    });
    expect(mcp.isError).toBe(true);
    expect(mcp.structuredContent).toMatchObject({ status: 403, detail: foreignFleetDetail });
    expect(await count()).toBe(before);
    await call(201, "/issues", "admiral", "POST", { ...fields, title: "Undispatched fixture" });
  });

  // Previews the move as the mover, because a preview token is bound to the member who asked.
  async function preview(role: Role, target: Issue, key: string) {
    const destinationProjectId = keys.get(key);
    const { token } = await call<{ token: string }>(
      200,
      `/issues/${target.id}/move-preview`,
      role,
      "POST",
      { destinationProjectId, version: target.version },
    );
    return { destinationProjectId, version: target.version, token };
  }
  const relocate = async (role: Role, target: Issue, key: string) =>
    http(`/issues/${target.id}/move`, role, "POST", await preview(role, target, key));
  async function moved(response: Response, key: string) {
    expect(response.status).toBe(200);
    const { issue: result } = (await response.json()) as { issue: Issue };
    expect(result.projectId).toBe(keys.get(key)!);
    return result;
  }

  test("a fleet agent moves issues between its own fleet's projects and projects without a fleet", async () => {
    let target = await issue("OWN", "Own move fixture");
    for (const key of ["KIN", "FREE", "LONE", "OWN"])
      target = await moved(await relocate("admiral", target, key), key);
  });

  test("a fleet agent cannot move an issue out of or into another fleet's project", async () => {
    const foreign = await issue("FOE", "Foreign move fixture");
    await refused(await relocate("admiral", foreign, "FREE"), foreignFleetMoveDetail);
    const mcp = await tool<{ status: number; detail: string }>("subspace_move_issue", {
      issue: foreign.id,
      ...(await preview("admiral", foreign, "OWN")),
    });
    expect(mcp.isError).toBe(true);
    expect(mcp.structuredContent).toMatchObject({ status: 403, detail: foreignFleetMoveDetail });
    expect(await read(foreign.id)).toMatchObject({
      identifier: foreign.identifier,
      projectId: keys.get("FOE"),
      version: foreign.version,
    });
    const own = await issue("OWN", "Outbound move fixture");
    await refused(await relocate("admiral", own, "FOE"), foreignFleetMoveDetail);
    expect(await read(own.id)).toMatchObject({
      identifier: own.identifier,
      projectId: keys.get("OWN"),
      version: own.version,
    });
  });

  test("humans and agents without a fleet move issues across fleets", async () => {
    await moved(await relocate("member", await issue("FOE", "Human move fixture"), "FREE"), "FREE");
    await moved(
      await relocate("drifter", await issue("OWN", "Fleetless move fixture"), "FOE"),
      "FOE",
    );
  });

  test("comments on another fleet's issue stay open to fleet agents", async () => {
    const target = await issue("FOE", "Cross-fleet discussion fixture");
    await call(201, `/issues/${target.id}/comments`, "admiral", "POST", {
      body: "A note from the alpha fleet.",
    });
  });

  interface Placed {
    id: string;
    parentId: string | null;
    version: number;
  }
  const placed = (id: string) => call<Placed>(200, `/initiatives/${id}`, "admin");
  async function relink(role: Role, id: string, parentId: string | null) {
    const { version } = await placed(id);
    return http(`/initiatives/${id}`, role, "PATCH", { version, parentId });
  }
  async function relinked(response: Response, parentId: string | null) {
    expect(response.status).toBe(200);
    expect(((await response.json()) as Placed).parentId).toBe(parentId);
  }
  async function unmoved(id: string, attempts: (string | null)[]) {
    const before = await placed(id);
    for (const parentId of attempts)
      await refused(await relink("admiral", id, parentId), foreignFleetReparentDetail);
    expect(await placed(id)).toEqual(before);
  }

  test("an agent that owns another fleet's initiative cannot reparent it out of that fleet", async () => {
    const betaProgramme = programmes.get("Beta programme")!;
    const alphaProgramme = programmes.get("Alpha programme")!;
    const free = programmes.get("Unfleeted programme")!;
    const inherited = await programme("Beta outpost", null, betaProgramme, "admiral");
    await unmoved(inherited, [null, free, alphaProgramme]);
    const own = await programme("Beta flagship", fleets.beta, undefined, "admiral");
    await unmoved(own, [free, alphaProgramme]);
  });

  test("an agent cannot reparent an initiative into another fleet's initiative", async () => {
    const betaProgramme = programmes.get("Beta programme")!;
    await unmoved(
      await programme("Alpha outpost", null, programmes.get("Alpha programme"), "admiral"),
      [betaProgramme],
    );
    await unmoved(await programme("Loose outpost", null, undefined, "admiral"), [betaProgramme]);
  });

  test("an agent reparents initiatives between its own fleet's initiatives and initiatives without a fleet", async () => {
    const roaming = await programme("Roaming outpost", null, undefined, "admiral");
    for (const name of ["Alpha programme", "Unfleeted programme", null, "Alpha workstream"]) {
      const parentId = name ? programmes.get(name)! : null;
      await relinked(await relink("admiral", roaming, parentId), parentId);
    }
  });

  test("humans and agents without a fleet reparent initiatives across fleets", async () => {
    const betaProgramme = programmes.get("Beta programme")!;
    const alphaProgramme = programmes.get("Alpha programme")!;
    const human = await programme("Human outpost", null, betaProgramme);
    await relinked(await relink("admin", human, null), null);
    await relinked(await relink("admin", human, betaProgramme), betaProgramme);
    await relinked(await relink("admin", human, alphaProgramme), alphaProgramme);
    const drifting = await programme("Drifter outpost", null, betaProgramme, "drifter");
    await relinked(await relink("drifter", drifting, alphaProgramme), alphaProgramme);
  });

  test("a fleet agent cannot set or clear an initiative's fleet", async () => {
    const owned = await programme("Unassigned outpost", null, undefined, "admiral");
    for (const fleetId of [fleets.alpha, null]) {
      const response = await http(`/initiatives/${owned}/fleet`, "admiral", "PUT", { fleetId });
      expect(response.status).toBe(403);
      expect(((await response.json()) as { detail: string }).detail).toBe(
        "Fleet assignment requires organisation admin access.",
      );
    }
    expect(
      (await call<{ fleetId: string | null }>(200, `/initiatives/${owned}`, "admin")).fleetId,
    ).toBeNull();
  });

  // Polls until a backend in the database waits on a row lock.
  async function waiting(client: pg.Client) {
    for (let attempt = 0; attempt < 50; attempt++) {
      const { rows: blocked } = await client.query(
        "select count(*)::int as count from pg_stat_activity where wait_event_type = 'Lock' and datname = current_database()",
      );
      if (blocked[0].count > 0) return true;
      await Bun.sleep(100);
    }
    return false;
  }

  // Holds an uncommitted ownership change in the API's own database while the agent writes. The
  // write must wait for the change and then enforce it, rather than read the committed state.
  test.skipIf(!testDatabase)(
    "a dispatch waits for a concurrent fleet move and then enforces it",
    async () => {
      const client = new pg.Client(localDatabase(testDatabase!).settings());
      await client.connect();
      try {
        const { rows } = await client.query("select 1 from projects where id = $1", [
          keys.get("RACE"),
        ]);
        if (!rows.length)
          throw new Error("TEST_DATABASE_URL must name the database that TEST_API_URL serves.");
        const race = async (key: string, change: string, values: unknown[], body: object) => {
          const target = await issue(key, `Raced ${key} fixture`);
          await client.query("begin");
          try {
            await client.query(change, values);
            const pending = edit("admiral", target, body);
            expect(await waiting(client)).toBe(true);
            await client.query("commit");
            await refused(await pending);
          } catch (error) {
            await client.query("rollback");
            throw error;
          }
          expect((await read(target.id)).version).toBe(target.version);
        };
        await race(
          "RACE",
          "update projects set initiative_id = $1 where id = $2",
          [programmes.get("Beta programme"), keys.get("RACE")],
          { assigneeId: captainId },
        );
        await race(
          "TIDE",
          "update initiatives set fleet_id = $1 where id = $2",
          [fleets.beta, programmes.get("Tidal programme")],
          { readyForAgents: true },
        );
      } finally {
        await client.end();
      }
    },
    20_000,
  );

  // Holds an uncommitted fleet assignment on the new parent while the agent reparents under it.
  test.skipIf(!testDatabase)(
    "a reparent waits for a concurrent fleet assignment on the new parent and then enforces it",
    async () => {
      const harbour = await programme("Harbour programme", null);
      const outpost = await programme("Harbour outpost", null, undefined, "admiral");
      const before = await placed(outpost);
      const client = new pg.Client(localDatabase(testDatabase!).settings());
      await client.connect();
      try {
        await client.query("begin");
        try {
          await client.query("update initiatives set fleet_id = $1 where id = $2", [
            fleets.beta,
            harbour,
          ]);
          const pending = http(`/initiatives/${outpost}`, "admiral", "PATCH", {
            version: before.version,
            parentId: harbour,
          });
          expect(await waiting(client)).toBe(true);
          await client.query("commit");
          await refused(await pending, foreignFleetReparentDetail);
        } catch (error) {
          await client.query("rollback");
          throw error;
        }
      } finally {
        await client.end();
      }
      expect(await placed(outpost)).toEqual(before);
    },
    20_000,
  );
});
