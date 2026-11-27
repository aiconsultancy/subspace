import { afterAll, beforeEach, expect, test } from "bun:test";
import { BunHttpServer } from "@effect/platform-bun";
import { Effect, Exit, Layer } from "effect";
import { HttpRouter } from "effect/http";
import { HttpApi, HttpApiBuilder } from "effect/http-api";
import { Database } from "../src/db";
import { CurrentMember, type Member } from "../src/identity";
import { issueGroup, issuesHandlers } from "../src/issues-http";
import { clearReadyForAgents, isTerminalState, readyForAgentsAfter } from "../src/issues";
import { mcpTools } from "../src/mcp-tools";
import { BearerAuth, validationErrors } from "../src/middleware";

const org = "00000000-0000-4000-8000-000000000001";
const assigneeId = "00000000-0000-4000-8000-000000000002";
const other = "00000000-0000-4000-8000-000000000003";
const created = new Date("2026-11-02T09:00:00.000Z");

const open = { tracker: false, terminal: false };
const run = (
  current: { assigneeId: string | null; readyForAgents: boolean } | null,
  input: { assigneeId?: string | null; readyForAgents?: boolean },
  result = open,
) => Effect.runSyncExit(readyForAgentsAfter(current, input, result));

function refused(
  exit: ReturnType<typeof run>,
  detail = "Remove the assignee before marking the issue ready for agents.",
) {
  expect(Exit.isFailure(exit)).toBe(true);
  if (Exit.isFailure(exit)) expect(JSON.stringify(exit.cause)).toContain(detail);
}

test("a new issue defaults to not ready and may be flagged only without an assignee", () => {
  expect(run(null, {})).toEqual(Exit.succeed({ readyForAgents: false, changed: false }));
  expect(run(null, { readyForAgents: true })).toEqual(
    Exit.succeed({ readyForAgents: true, changed: false }),
  );
  expect(run(null, { assigneeId: null, readyForAgents: true })).toEqual(
    Exit.succeed({ readyForAgents: true, changed: false }),
  );
  refused(run(null, { assigneeId, readyForAgents: true }));
  expect(run(null, { assigneeId, readyForAgents: false })).toEqual(
    Exit.succeed({ readyForAgents: false, changed: false }),
  );
});

test("an edit cannot flag an issue that keeps or gains an assignee", () => {
  refused(run({ assigneeId, readyForAgents: false }, { readyForAgents: true }));
  refused(run({ assigneeId: null, readyForAgents: false }, { assigneeId, readyForAgents: true }));
  refused(run({ assigneeId, readyForAgents: false }, { assigneeId: other, readyForAgents: true }));
});

test("clearing the assignee and flagging in one edit is allowed", () => {
  expect(
    run({ assigneeId, readyForAgents: false }, { assigneeId: null, readyForAgents: true }),
  ).toEqual(Exit.succeed({ readyForAgents: true, changed: true }));
});

test("any write that leaves an assignee clears the flag and reports the change", () => {
  expect(run({ assigneeId: null, readyForAgents: true }, { assigneeId })).toEqual(
    Exit.succeed({ readyForAgents: false, changed: true }),
  );
  expect(
    run({ assigneeId: null, readyForAgents: true }, { assigneeId, readyForAgents: false }),
  ).toEqual(Exit.succeed({ readyForAgents: false, changed: true }));
  expect(run({ assigneeId: null, readyForAgents: false }, { assigneeId })).toEqual(
    Exit.succeed({ readyForAgents: false, changed: false }),
  );
});

test("clearing the assignee does not set the flag, and unrelated edits keep it", () => {
  expect(run({ assigneeId, readyForAgents: false }, { assigneeId: null })).toEqual(
    Exit.succeed({ readyForAgents: false, changed: false }),
  );
  expect(run({ assigneeId: null, readyForAgents: true }, {})).toEqual(
    Exit.succeed({ readyForAgents: true, changed: false }),
  );
  expect(run({ assigneeId: null, readyForAgents: true }, { readyForAgents: false })).toEqual(
    Exit.succeed({ readyForAgents: false, changed: true }),
  );
});

test("a tracker can never be flagged, and retyping an issue to a tracker clears the flag", () => {
  const tracker = { tracker: true, terminal: false };
  const detail = "Trackers cannot be marked ready for agents.";
  refused(run(null, { readyForAgents: true }, tracker), detail);
  refused(
    run({ assigneeId: null, readyForAgents: false }, { readyForAgents: true }, tracker),
    detail,
  );
  expect(run({ assigneeId: null, readyForAgents: true }, {}, tracker)).toEqual(
    Exit.succeed({ readyForAgents: false, changed: true }),
  );
  expect(run(null, { readyForAgents: false }, tracker)).toEqual(
    Exit.succeed({ readyForAgents: false, changed: false }),
  );
});

test("a terminal issue cannot be flagged, and closing a flagged issue clears the flag", () => {
  const closed = { tracker: false, terminal: true };
  const detail = "Reopen the issue before marking it ready for agents.";
  refused(run(null, { readyForAgents: true }, closed), detail);
  refused(
    run({ assigneeId: null, readyForAgents: false }, { readyForAgents: true }, closed),
    detail,
  );
  expect(run({ assigneeId: null, readyForAgents: true }, {}, closed)).toEqual(
    Exit.succeed({ readyForAgents: false, changed: true }),
  );
  // Reopening does not set the flag again.
  expect(run({ assigneeId: null, readyForAgents: false }, {})).toEqual(
    Exit.succeed({ readyForAgents: false, changed: false }),
  );
});

test("only completed, canceled and duplicate states are terminal", () => {
  expect(["completed", "canceled", "duplicate"].every(isTerminalState)).toBe(true);
  expect(["triage", "backlog", "unstarted", "started"].some(isTerminalState)).toBe(false);
});

test("side writes clear a set flag and name it, and leave an unset flag alone", () => {
  expect(clearReadyForAgents({ readyForAgents: true }, true)).toEqual({
    set: { readyForAgents: false },
    fields: ["readyForAgents"],
  });
  expect(clearReadyForAgents({ readyForAgents: false }, true)).toEqual({ set: {}, fields: [] });
  expect(clearReadyForAgents({ readyForAgents: true }, false)).toEqual({ set: {}, fields: [] });
});

test("MCP issue tools accept readyForAgents in the same shapes as REST", () => {
  const properties = (schema: unknown) =>
    /"readyForAgents":\{"anyOf":\[\{"type":"boolean"\}/u.test(JSON.stringify(schema));
  expect(properties(mcpTools.subspace_create_issue.inputSchema)).toBe(true);
  expect(properties(mcpTools.subspace_update_issue.inputSchema)).toBe(true);
  expect(properties(mcpTools.subspace_list_issues.inputSchema)).toBe(true);
});

// Serves the real issue routes with a stub database that answers every transaction with `reply`
// and records any other access, so query decoding and response encoding need no PostgreSQL.
let reply: unknown;
const calls: string[] = [];
const database = new Proxy(
  {},
  {
    get: (_target, name) => {
      calls.push(String(name));
      if (name === "transaction") return () => Effect.succeed(reply);
      return () => {
        throw new Error("The stub database has no rows.");
      };
    },
  },
);
const human: Member = {
  id: other,
  organisationId: org,
  subject: "lead@example.com",
  email: "lead@example.com",
  displayName: "Example Lead",
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
        ...human,
        tokenExpiresAt: 0,
        roleSource: "provisioned",
        tokenGrant: null,
      }),
  }),
);
const api = HttpApi.make("ready-for-agents-test").add(issueGroup);
const app = HttpApiBuilder.layer(api).pipe(
  Layer.provide(HttpApiBuilder.group(api, "issues", issuesHandlers)),
  Layer.provide(Layer.mergeAll(auth, validationErrors)),
  HttpRouter.provideRequest(Layer.succeed(Database, database as (typeof Database)["Service"])),
  Layer.provide(BunHttpServer.layerHttpServices),
);
const { handler, dispose } = HttpRouter.toWebHandler(app, { disableLogger: true });
afterAll(() => dispose());
beforeEach(() => {
  calls.length = 0;
  reply = undefined;
});

const send = (path: string, method = "GET", body?: unknown) =>
  handler(
    new Request(`http://127.0.0.1/api/v1${path}`, {
      method,
      headers: { Authorization: "Bearer example", "Content-Type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    }),
  );

const item = (readyForAgents: boolean) => ({
  id: "00000000-0000-4000-8000-000000000010",
  identifier: "EXA-1",
  projectId: "00000000-0000-4000-8000-000000000011",
  projectKey: "EXA",
  type: "issue",
  title: "Example issue",
  summary: null,
  state: { id: "00000000-0000-4000-8000-000000000012", type: "unstarted", name: "Todo" },
  assignee: null,
  readyForAgents,
  owner: null,
  priority: 0,
  estimate: null,
  dueDate: null,
  labels: [],
  milestoneId: null,
  parentId: null,
  blocked: false,
  archivedAt: null,
  createdAt: created.toISOString(),
  updatedAt: created.toISOString(),
  version: 1,
  progress: null,
  commentCount: 0,
  todoCounts: { total: 0, done: 0 },
  lastActivityAt: created.toISOString(),
  lastActivity: null,
  relayedCrew: null,
  provenance: null,
  cycle: null,
  cycleId: null,
  pullRequestSummary: null,
});

test("list items carry readyForAgents and the filter accepts true and false", async () => {
  reply = { items: [item(true)], nextCursor: null };
  for (const query of ["readyForAgents=true", "readyForAgents=false&assignee=none"]) {
    const response = await send(`/issues?${query}`);
    expect([query, response.status]).toEqual([query, 200]);
    const body = (await response.json()) as { items: Record<string, unknown>[] };
    expect(body.items[0]).toMatchObject({ readyForAgents: true, assignee: null });
  }
  reply = { groupBy: "stateType", groups: [] };
  expect((await send("/issues/groups?groupBy=stateType&readyForAgents=true")).status).toBe(200);
});

test("an invalid readyForAgents filter fails like the other boolean filters", async () => {
  const flag = await send("/issues?readyForAgents=maybe");
  const subscribed = await send("/issues?subscribed=maybe");
  expect(flag.status).toBe(400);
  expect(subscribed.status).toBe(400);
  const [flagBody, subscribedBody] = (await Promise.all([flag.json(), subscribed.json()])) as {
    title: string;
    detail: string;
  }[];
  expect(flagBody!.title).toBe(subscribedBody!.title);
  expect(flagBody!.detail.replace("readyForAgents", "subscribed")).toBe(subscribedBody!.detail);
  expect(calls).toEqual([]);
});

test("issue writes reject a non-boolean readyForAgents before touching the database", async () => {
  const create = await send("/issues", "POST", {
    projectId: "00000000-0000-4000-8000-000000000011",
    title: "Example issue",
    summary: "Example",
    readyForAgents: "yes",
  });
  expect(create.status).toBe(400);
  const edit = await send("/issues/EXA-1", "PATCH", { version: 1, readyForAgents: 1 });
  expect(edit.status).toBe(400);
  expect(calls).toEqual([]);
});

test("the issue contract requires readyForAgents on every list item", async () => {
  const { readyForAgents: _omitted, ...missing } = item(false);
  reply = { items: [missing], nextCursor: null };
  const response = await send("/issues");
  expect(response.status).not.toBe(200);
  expect(await response.text()).not.toContain("Example issue");
});
