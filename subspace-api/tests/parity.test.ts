import { beforeAll, describe, expect, test } from "bun:test";
import { createHmac } from "node:crypto";

const origin = process.env.TEST_API_URL?.replace(/\/$/u, "");
const webhookSecret = process.env.TEST_GITHUB_WEBHOOK_SECRET;
const suffix = crypto.randomUUID().slice(0, 8);
const fixtureOrg = process.env.TEST_GITHUB_ORGANISATION;
const organisation = fixtureOrg ?? `parity-${suffix}`;

interface Page<T> {
  items: T[];
  nextCursor: string | null;
}
interface Versioned {
  id: string;
  version: number;
}
interface Issue extends Versioned {
  identifier: string;
  state: { id: string; type: string };
}
type TokenRole = "admin" | "member" | "other" | "captain" | "crew" | "rearAdmiral";
const tokens = new Map<TokenRole, string>();
const members = new Map<TokenRole, string>();
let projectId: string;
let initiativeId: string;
let fleetId: string;

async function http(
  path: string,
  role?: TokenRole,
  method = "GET",
  data?: unknown,
  extra?: HeadersInit,
) {
  const headers = new Headers(extra);
  const token = role ? tokens.get(role) : undefined;
  if (token) headers.set("Authorization", `Bearer ${token}`);
  if (data !== undefined) headers.set("Content-Type", "application/json");
  return fetch(`${origin}${path.startsWith("/mcp") ? "" : "/api/v1"}${path}`, {
    method,
    headers,
    body: data === undefined ? undefined : JSON.stringify(data),
    signal: AbortSignal.timeout(5000),
  });
}

async function call<T>(
  expected: number,
  path: string,
  role: TokenRole = "admin",
  method = "GET",
  data?: unknown,
) {
  const response = await http(path, role, method, data);
  if (response.status !== expected) {
    const problem = (await response
      .json()
      .catch(() => ({ detail: "Unexpected response body" }))) as { detail?: string };
    throw new Error(
      `${method} ${path}: received ${response.status}, expected ${expected}. ${problem.detail ?? ""}`,
    );
  }
  return (response.status === 204 ? undefined : await response.json()) as T;
}

async function signIn(role: TokenRole, email: string, org = organisation) {
  const response = await http("/auth/dev/token", undefined, "POST", { email, organisation: org });
  expect(response.status).toBe(200);
  const { accessToken } = (await response.json()) as { accessToken: string };
  tokens.set(role, accessToken);
  members.set(role, (await call<{ userId: string }>(200, "/me", role)).userId);
}

function createIssue(title: string, role: TokenRole = "admin", fields: object = {}) {
  return call<Issue>(201, "/issues", role, "POST", {
    projectId,
    title,
    summary: "Fixture issue",
    ...fields,
  });
}

function agentInput(rank: "rearAdmiral" | "captain" | "crew") {
  return {
    name: `Fixture-${rank}`,
    rank,
    fleet: "example-fleet",
    ship: rank === "rearAdmiral" ? null : "Example ship",
    defaultModel: "example-model",
    serviceTokenClientId: `${rank.toLowerCase()}@example.com`,
  };
}

const mcpHeaders = {
  Accept: "application/json, text/event-stream",
  "MCP-Protocol-Version": "2025-11-25",
};
async function rpc<T>(role: TokenRole, method: string, params = {}) {
  const response = await http(
    "/mcp",
    role,
    "POST",
    { jsonrpc: "2.0", id: 1, method, params },
    mcpHeaders,
  );
  expect(response.status).toBe(200);
  return response.json() as Promise<{ result?: T; error?: { code: number } }>;
}
async function tool<T>(name: string, args: object, role: TokenRole = "captain") {
  const result = await rpc<{ structuredContent: T; isError: boolean }>(role, "tools/call", {
    name,
    arguments: args,
  });
  expect(result.error).toBeUndefined();
  return result.result!;
}

async function hook(event: string, data: object, valid = true) {
  const body = JSON.stringify(data);
  const digest = createHmac("sha256", valid ? webhookSecret! : "different-fixture")
    .update(body)
    .digest("hex");
  return fetch(`${origin}/api/v1/integrations/github/webhook`, {
    method: "POST",
    body,
    headers: { "X-GitHub-Event": event, "X-Hub-Signature-256": `sha256=${digest}` },
    signal: AbortSignal.timeout(5000),
  });
}

const pull = (
  repository: string,
  body: string,
  mergedAt: string | null,
  number = 31,
  updatedAt = new Date().toISOString(),
) => ({
  repository: { full_name: repository },
  pull_request: {
    number,
    title: "Fixture pull request",
    body,
    user: { login: "operator" },
    head: { ref: "example-change" },
    base: { ref: "main" },
    state: mergedAt ? "closed" : "open",
    draft: false,
    merged_at: mergedAt,
    closed_at: mergedAt,
    updated_at: updatedAt,
  },
});

describe.skipIf(!origin)("round two HTTP contracts", () => {
  beforeAll(async () => {
    await signIn("admin", "admin@example.com");
    await signIn("member", "member@example.com");
    await signIn("other", "admin@example.com", `other-${suffix}`);
    projectId = (
      await call<Versioned>(201, "/projects", "admin", "POST", {
        key: "PAR",
        name: "Parity fixture",
      })
    ).id;
    const mine = await call<Page<{ id: string; fleetId: string }>>(200, "/me/initiatives");
    initiativeId = mine.items[0]!.id;
    fleetId = mine.items[0]!.fleetId;
    for (const rank of ["captain", "crew", "rearAdmiral"] as const) {
      await call(201, "/agents", "admin", "POST", agentInput(rank));
      await signIn(rank, agentInput(rank).serviceTokenClientId);
    }
  });

  test.each(["captain", "crew", "rearAdmiral"] as const)(
    "%s uses the provider subject as a distinct agent identity",
    async (rank) => {
      expect(await call(200, "/me", rank)).toMatchObject({
        kind: "agent",
        agentRank: rank,
        userId: members.get(rank),
      });
      const issue = await createIssue(`Agent issue from ${rank}`, rank);
      expect(await call(200, `/issues/${issue.id}`, rank)).toMatchObject({
        createdBy: { userId: members.get(rank), kind: "agent" },
      });
      await call(403, "/agents", rank, "POST", agentInput(rank));
      await call(403, `/initiatives/${initiativeId}/fleet`, rank, "PUT", { fleetId });
    },
  );

  test("fleet and ship links by ID and name follow the roster and hardware names", async () => {
    const fleet = {
      slug: `roster-${suffix}`,
      name: "Example fleet",
      ircHost: "irc.example.test",
      ircPort: 6697,
      ircTls: true,
    };
    await call(403, "/fleets", "member", "POST", fleet);
    await call(400, "/fleets", "admin", "POST", { ...fleet, rearAdmiralName: "Ignored name" });
    const registered = await call<Versioned>(201, "/fleets", "admin", "POST", fleet);
    await call(409, "/fleets", "admin", "POST", fleet);
    await call(403, `/fleets/${registered.id}/ships`, "captain", "POST", { name: "Forbidden" });
    const ship = await call<Versioned>(201, `/fleets/${registered.id}/ships`, "member", "POST", {
      name: "Aurora",
    });
    await call(409, `/fleets/${registered.id}/ships`, "admin", "POST", { name: "aUrOrA" });
    const captain = {
      ...agentInput("captain"),
      name: "Zeta-captain",
      serviceTokenClientId: "zeta@example.test",
    };
    await call(400, "/agents", "admin", "POST", { ...captain, fleetId, shipId: ship.id });
    const linked = await call<{ userId: string }>(201, "/agents", "admin", "POST", {
      ...captain,
      fleetId: registered.id,
      shipId: ship.id,
    });
    await call(201, "/agents", "admin", "POST", {
      ...captain,
      name: "Alpha-captain",
      serviceTokenClientId: "alpha@example.test",
      fleet: fleet.slug,
      ship: "zenith",
    });
    await call(201, "/agents", "admin", "POST", {
      ...agentInput("rearAdmiral"),
      name: "Roster-admiral",
      serviceTokenClientId: "roster-admiral@example.test",
      fleetId: registered.id,
    });
    await call(200, `/ships/${ship.id}`, "member", "PATCH", {
      version: ship.version,
      name: "Borealis",
    });
    await call(409, `/ships/${ship.id}`, "admin", "PATCH", {
      version: ship.version,
      name: "Stale",
    });
    expect(await call(200, `/agents/${linked.userId}`)).toMatchObject({
      fleetId: registered.id,
      fleet: fleet.slug,
      shipId: ship.id,
      ship: "Borealis",
    });
    const signInResponse = await http("/auth/dev/token", undefined, "POST", {
      email: captain.serviceTokenClientId,
      organisation,
    });
    expect(signInResponse.status).toBe(200);
    const { accessToken } = (await signInResponse.json()) as { accessToken: string };
    const profile = await http("/me", undefined, "GET", undefined, {
      Authorization: `Bearer ${accessToken}`,
    });
    expect(profile.status).toBe(200);
    expect(await profile.json()).toMatchObject({
      fleetId: registered.id,
      fleet: fleet.slug,
      shipId: ship.id,
      ship: "Borealis",
    });
    expect(await call(200, `/fleets/${fleet.slug}`)).toMatchObject({
      rearAdmiralName: "Roster-admiral",
      captainNames: ["Alpha-captain", "Zeta-captain"],
      agents: [
        { name: "Roster-admiral", rank: "rearAdmiral", shipId: null, ship: null },
        { name: "Alpha-captain", rank: "captain", ship: "zenith" },
        { name: "Zeta-captain", rank: "captain", shipId: ship.id, ship: "Borealis" },
      ],
      ships: [{ id: ship.id, name: "Borealis" }, { name: "zenith" }],
    });
    expect(await call(200, `/fleets/${registered.id}/ships`, "member")).toMatchObject([
      { id: ship.id, fleetId: registered.id, name: "Borealis" },
      { fleetId: registered.id, name: "zenith" },
    ]);
    await call(404, `/fleets/${registered.id}/ships`, "other");
    await call(403, "/initiatives", "member", "POST", {
      name: "Forbidden",
      ownerId: members.get("member"),
    });
  });

  test("only command ranks may relay crew work, and audit keeps the authenticated actor", async () => {
    const relayedCrew = { name: "Example crew", model: "example-model" };
    for (const role of ["member", "crew"] as const) {
      await call(400, "/issues", role, "POST", {
        projectId,
        title: "Rejected relay",
        summary: "Fixture",
        relayedCrew,
      });
    }
    const issue = await createIssue("Relayed work fixture", "captain", { relayedCrew });
    expect(await call(200, `/issues/${issue.id}`)).toMatchObject({
      relayedCrew,
      createdBy: { userId: members.get("captain") },
    });
    const comment = await tool<{
      relayedCrew: typeof relayedCrew;
      actualAuthor: { userId: string };
    }>(
      "subspace_create_comment",
      { issue: issue.id, body: "Relayed comment", relayedCrew },
      "rearAdmiral",
    );
    expect(comment.structuredContent).toMatchObject({
      relayedCrew,
      actualAuthor: { userId: members.get("rearAdmiral") },
    });
    expect(await call(200, `/audit?entityId=${issue.id}`)).toMatchObject({
      items: expect.arrayContaining([
        expect.objectContaining({
          actorId: members.get("captain"),
          relayedCrewName: relayedCrew.name,
        }),
        expect.objectContaining({
          actorId: members.get("rearAdmiral"),
          relayedCrewModel: relayedCrew.model,
          source: "mcp",
        }),
      ]),
    });
  });

  test("ordinary member cannot register agents, replace repositories or modify project requirements", async () => {
    await call(403, "/agents", "member", "POST", agentInput("captain"));
    await call(403, `/fleets/${fleetId}`, "member", "PATCH", { ircHost: "other.example.test" });
    const project = await call<Versioned>(200, `/projects/${projectId}`);
    await call(403, `/projects/${projectId}/repositories`, "member", "PUT", {
      version: project.version,
      repositories: [],
    });
    await call(403, `/projects/${projectId}/review-requirements`, "member", "PUT", {
      version: project.version,
      kinds: {},
    });
  });

  test("registration cannot take over another subject and deactivation revokes existing JWTs", async () => {
    await call(409, "/agents", "admin", "POST", {
      ...agentInput("captain"),
      serviceTokenClientId: "member@example.com",
      name: "Duplicate-subject-fixture",
    });
    await call(400, "/agents", "admin", "POST", { ...agentInput("captain"), ship: null });
    const id = members.get("captain");
    const agent = await call<{ version: number }>(200, `/agents/${id}`);
    await call(404, `/agents/${id}`, "other");
    const disabled = await call<{ version: number }>(
      200,
      `/agents/${id}/deactivate`,
      "admin",
      "POST",
      { version: agent.version },
    );
    await call(403, "/me", "captain");
    await call(409, `/agents/${id}/reactivate`, "admin", "POST", { version: agent.version });
    await call(200, `/agents/${id}/reactivate`, "admin", "POST", { version: disabled.version });
    await call(200, "/me", "captain");
  });

  test("MCP negotiates HTTP, validates origin, and discovers the issue tools", async () => {
    const unauthorised = await http(
      "/mcp",
      undefined,
      "POST",
      { jsonrpc: "2.0", id: 1, method: "tools/list" },
      mcpHeaders,
    );
    expect(unauthorised.status).toBe(401);
    const browser = await http(
      "/mcp",
      "captain",
      "POST",
      { jsonrpc: "2.0", id: 1, method: "tools/list" },
      { ...mcpHeaders, Origin: "https://example.com" },
    );
    expect(browser.status).toBe(403);
    const init = await rpc<{ protocolVersion: string }>("captain", "initialize", {
      protocolVersion: "2025-11-25",
    });
    expect(init.result?.protocolVersion).toBe("2025-11-25");
    expect(
      (
        await http(
          "/mcp",
          "captain",
          "POST",
          { jsonrpc: "2.0", method: "notifications/initialized" },
          mcpHeaders,
        )
      ).status,
    ).toBe(202);
    expect((await http("/mcp", "captain")).status).toBe(405);
    const tools = await rpc<{
      tools: {
        name: string;
        inputSchema: { type: string };
        annotations: { readOnlyHint: boolean };
      }[];
    }>("captain", "tools/list");
    expect(tools.result?.tools).toContainEqual(
      expect.objectContaining({
        name: "subspace_get_issue",
        annotations: expect.objectContaining({ readOnlyHint: true }),
      }),
    );
    expect(tools.result?.tools.every((entry) => entry.inputSchema.type === "object")).toBe(true);
    expect((await rpc("captain", "tools/call", { name: "unregistered" })).error?.code).toBe(-32602);
  });

  test("MCP creates, updates and lists issues with the same version and access semantics as REST", async () => {
    const created = await tool<Issue>("subspace_create_issue", {
      project: "PAR",
      title: "MCP fixture",
      summary: "Written by a captain",
    });
    expect(created.isError).toBe(false);
    const issue = created.structuredContent;
    const changed = await tool<Issue>("subspace_update_issue", {
      issue: issue.identifier,
      changes: { version: issue.version, title: "MCP updated fixture" },
    });
    expect(changed.structuredContent.version).toBe(issue.version + 1);
    expect(await call(200, `/issues/${issue.id}`)).toMatchObject({ title: "MCP updated fixture" });
    const stale = await tool<{ status: number; currentVersion: number }>("subspace_update_issue", {
      issue: issue.id,
      changes: { version: issue.version, title: "Stale" },
    });
    expect(stale.isError).toBe(true);
    expect(stale.structuredContent).toMatchObject({
      status: 409,
      currentVersion: issue.version + 1,
    });
    const foreign = await tool<{ status: number }>(
      "subspace_get_issue",
      { issue: issue.id },
      "other",
    );
    expect(foreign.structuredContent.status).toBe(404);
    const rejected = await tool<{ status: number }>("subspace_update_issue", {
      issue: issue.id,
      changes: { title: "Unversioned" },
    });
    expect(rejected.structuredContent.status).toBe(400);
    expect(
      (
        await tool<Page<Issue>>("subspace_list_issues", {
          filters: { project: "PAR", q: "MCP updated", limit: 1 },
        })
      ).structuredContent.items[0]?.id,
    ).toBe(issue.id);
    const audit = await call<Page<{ source: string; actorType: string }>>(
      200,
      `/audit?project=PAR&entityId=${issue.id}`,
      "captain",
    );
    expect(audit.items).toContainEqual(
      expect.objectContaining({ source: "mcp", actorType: "agent" }),
    );
    expect((await call<Page<unknown>>(200, "/audit?entityType=Agent", "captain")).items).toEqual(
      [],
    );
    expect((await call<Page<unknown>>(200, `/audit?entityId=${issue.id}`, "other")).items).toEqual(
      [],
    );
  });

  test("stakeholders accept both handle types, appear in initiative reads and preserve fleet assignment", async () => {
    const path = `/initiatives/${initiativeId}/stakeholders`;
    const data = {
      name: "Example reviewer",
      role: "Reviewer",
      slackHandle: "U-EXAMPLE",
      teamsHandle: "reviewer@example.com",
    };
    await call(403, path, "member", "POST", data);
    await call(403, path, "rearAdmiral", "POST", data);
    await call(404, path, "other", "POST", data);
    await call(400, path, "admin", "POST", { ...data, slackHandle: null, teamsHandle: null });
    const saved = await call<typeof data & { id: string }>(201, path, "admin", "POST", data);
    expect(saved).toMatchObject(data);
    expect(await call(200, `${path}/${saved.id}`, "member")).toEqual(saved);
    expect(await call<{ items: unknown[] }>(200, path)).toEqual({ items: [saved] });
    expect(await call(200, `/initiatives/${initiativeId}`)).toMatchObject({
      fleetId,
      stakeholders: [saved],
    });
    const list = await call<Page<{ id: string; stakeholders: unknown[] }>>(200, "/initiatives");
    expect(list.items.find(({ id }) => id === initiativeId)?.stakeholders).toEqual([saved]);
    const memberList = await call<Page<{ id: string; stakeholders: unknown[] }>>(
      200,
      "/me/initiatives",
    );
    expect(memberList.items.find(({ id }) => id === initiativeId)?.stakeholders).toEqual([saved]);
    await call(404, `${path}/${saved.id}`, "other");
    await call(400, `${path}/${saved.id}`, "admin", "PATCH", {
      slackHandle: null,
      teamsHandle: null,
    });
    expect(
      await call(200, `${path}/${saved.id}`, "admin", "PATCH", { slackHandle: null }),
    ).toMatchObject({ teamsHandle: data.teamsHandle, slackHandle: null });
    await call(204, `${path}/${saved.id}`, "admin", "DELETE");
    await call(404, `${path}/${saved.id}`);
  });

  test("initiative owners may write stakeholders and archived ancestors reject changes", async () => {
    const parent = await call<Versioned>(201, "/initiatives", "admin", "POST", {
      name: "Stakeholder parent",
      ownerId: members.get("admin"),
    });
    const child = await call<Versioned>(201, "/initiatives", "admin", "POST", {
      name: "Stakeholder child",
      ownerId: members.get("member"),
      parentId: parent.id,
    });
    const path = `/initiatives/${child.id}/stakeholders`;
    const input = { name: "Example stakeholder", role: "Owner", slackHandle: "U-EXAMPLE" };
    expect(await call(201, path, "member", "POST", input)).toMatchObject(input);
    await call(200, `/initiatives/${parent.id}/archive`, "admin", "POST", {
      version: parent.version,
    });
    await call(409, path, "admin", "POST", input);
  });

  test("mentions and comments deliver private notifications with working location links", async () => {
    const issue = await createIssue("In-app notification fixture");
    const replyPath = `/issues/${issue.id}/comments`;
    await call(200, `/issues/${issue.id}/subscription`, "member", "PUT", { subscribed: true });
    const comment = await call<{ id: string; mentions: unknown[] }>(
      201,
      replyPath,
      "admin",
      "POST",
      {
        body: `@[Example Member](member:${members.get("member")}) please review.`,
      },
    );
    expect(comment.mentions).toContainEqual(
      expect.objectContaining({ userId: members.get("member"), status: "canRead" }),
    );
    const page = await call<Page<{ id: string; issueId: string; type: string; actionUrl: string }>>(
      200,
      "/notifications?unreadOnly=true",
      "member",
    );
    const notice = page.items.find(({ issueId }) => issueId === issue.id)!;
    expect(notice.type).toBe("mentioned");
    expect(await call(200, notice.actionUrl.replace("/api/v1", ""), "member")).toMatchObject({
      root: { id: comment.id },
      replies: [],
    });
    await call(404, `/notifications/${notice.id}/read`, "admin", "POST");
    await call(404, `/notifications/${notice.id}/read`, "other", "POST");
    expect(await call(200, `/notifications/${notice.id}/read`, "member", "POST")).toMatchObject({
      isRead: true,
    });
    expect(await call(200, `/notifications/${notice.id}/unread`, "member", "POST")).toMatchObject({
      isRead: false,
    });
    expect(await call(200, "/notifications/read-all", "member", "POST")).toMatchObject({
      count: 1,
    });
    await call(400, replyPath, "admin", "POST", {
      body: `@[Outside](member:${members.get("other")})`,
    });
  });

  test("thread changes retain authorship, tombstones and optimistic versions", async () => {
    const issue = await createIssue("Thread fixture");
    const list = `/issues/${issue.id}/comments`;
    const root = await call<Versioned>(201, list, "member", "POST", { body: "Root comment" });
    const path = `${list}/${root.id}`;
    const reply = await call<Versioned>(201, list, "captain", "POST", {
      body: "Reply comment",
      parentCommentId: root.id,
    });
    await call(404, path, "other");
    await call(403, path, "admin", "PUT", { version: root.version, body: "Operator edit" });
    await call(403, `${path}/delete`, "captain", "POST", { version: root.version });
    const saved = await call<Versioned>(200, path, "member", "PUT", {
      version: root.version,
      body: "Edited root",
    });
    await call(409, path, "member", "PUT", { version: root.version, body: "Stale edit" });
    expect(
      await call(200, `${path}/resolution`, "crew", "PUT", {
        version: saved.version,
        resolved: true,
      }),
    ).toMatchObject({ resolved: true, resolvedBy: { userId: members.get("crew") } });
    expect(await call(200, `/issue-comments/${reply.id}/location`)).toMatchObject({
      root: { id: root.id },
      replies: [{ id: reply.id }],
    });
    expect(await call(200, `${path}/delete`, "admin", "POST", {})).toMatchObject({
      body: "",
      replyCount: 1,
    });
    await call(409, path, "member", "PUT", { body: "Deleted comment" });
  });

  test("documents keep immutable revisions and reviews apply only to their selected revision", async () => {
    const issue = await createIssue("Document fixture");
    const project = await call<Versioned>(200, `/projects/${projectId}`);
    const requirements = `/projects/${projectId}/review-requirements`;
    await call(200, requirements, "admin", "PUT", {
      version: project.version,
      kinds: { spec: [{ model: "example-2", reasoning: "medium" }] },
    });
    await call(409, requirements, "admin", "PUT", { version: project.version, kinds: {} });
    const document = await call<{ id: string; reference: string }>(
      201,
      `/issues/${issue.id}/documents`,
      "crew",
      "POST",
      {
        type: "spec",
        title: "Fixture specification",
        content: "Version one",
      },
    );
    const path = `/documents/${document.reference}`;
    await call(404, `/documents/${document.id}`, "other");
    expect(await call(200, `${path}/review-status`)).toMatchObject({
      documentRevision: 1,
      allMet: false,
    });
    const review = await call<{ id: string }>(
      201,
      `/issues/${issue.id}/reviews`,
      "captain",
      "POST",
      {
        model: "provider-example-2-fast",
        reasoning: "high",
        verdict: "approved",
        document: document.reference,
        documentRevision: 1,
      },
    );
    expect(await call(200, `${path}/review-status`)).toMatchObject({
      allMet: true,
      requirements: [{ reviewId: review.id }],
    });
    expect(
      await call(200, path, "crew", "PUT", {
        baseRevision: 1,
        content: "Version two",
        note: "Updated",
      }),
    ).toMatchObject({ revision: 2 });
    await call(409, path, "crew", "PUT", { baseRevision: 1, content: "Stale" });
    expect(await call(200, `${path}/revisions/1`)).toMatchObject({ content: "Version one" });
    expect(await call(200, `${path}/review-status`)).toMatchObject({
      documentRevision: 2,
      allMet: false,
    });
    expect(await call(200, `${path}/review-status?revision=1`)).toMatchObject({ allMet: true });
    expect(
      await call(
        200,
        `/issues/${issue.id}/reviews?document=${document.reference}&documentRevision=1`,
      ),
    ).toMatchObject({ items: [{ id: review.id }] });
    const unrelated = await createIssue("Unrelated review fixture");
    await call(404, `/issues/${unrelated.id}/reviews`, "captain", "POST", {
      model: "example-2",
      verdict: "approved",
      document: document.id,
    });
    expect(await call(200, `${path}/archive`, "admin", "POST")).toMatchObject({ archived: true });
    expect(await call(200, `/issues/${issue.id}/documents`)).toMatchObject({ items: [] });
    expect(await call(200, `${path}/unarchive`, "admin", "POST")).toMatchObject({
      archived: false,
    });
  });

  test("decision edits preserve history and accepting a replacement supersedes the old decision", async () => {
    const first = await call<Versioned>(201, "/decisions", "admin", "POST", {
      projectId,
      title: "Original accepted record",
      decidedOn: "2026-10-01",
      decision: "Use version one",
      status: "accepted",
    });
    const next = await call<Versioned>(201, "/decisions", "captain", "POST", {
      projectId,
      title: "Proposed update",
      decidedOn: "2026-10-02",
      decision: "Use version two",
      supersedesId: first.id,
    });
    const path = `/decisions/${next.id}`;
    await call(403, path, "crew", "PUT", { title: "Unauthorised update", version: 1 });
    await call(404, `${path}/revisions`, "other");
    const edited = await call<Versioned>(200, path, "captain", "PUT", {
      version: next.version,
      title: "Edited proposed update",
    });
    await call(409, `${path}/status`, "admin", "PUT", {
      version: next.version,
      status: "accepted",
    });
    expect(
      await call(200, `${path}/status`, "admin", "PUT", {
        version: edited.version,
        status: "accepted",
      }),
    ).toMatchObject({ status: "accepted", supersedes: { id: first.id, status: "superseded" } });
    expect(await call(200, `/decisions/${first.id}`)).toMatchObject({
      status: "superseded",
      supersededById: next.id,
    });
    const revisions = await call<Page<{ id: string; number: number }>>(200, `${path}/revisions`);
    expect(revisions.items.map(({ number }) => number)).toEqual([3, 2, 1]);
    expect(await call(200, `${path}/revisions/${revisions.items[2]!.id}`)).toMatchObject({
      snapshot: { title: "Proposed update", status: "proposed" },
      currentVersion: 3,
    });
  });

  test("decision list items name their members and counts follow the list's filters and access", async () => {
    type Member = { userId: string; displayName: string; available: boolean; kind: string };
    type Decision = Versioned & {
      status: string;
      recordedBy: Member;
      madeBy: Member | null;
      madeByAgentName: string | null;
    };
    type Counts = Record<"proposed" | "accepted" | "rejected" | "superseded" | "total", number>;
    const project = await call<Versioned>(201, "/projects", "admin", "POST", {
      key: "CNT",
      name: "Counts fixture",
    });
    const issue = await createIssue("Counted decision issue", "admin", { projectId: project.id });
    const log = (role: TokenRole, fields: object) =>
      call<Decision>(201, "/decisions", role, "POST", {
        projectId: project.id,
        decidedOn: "2026-11-01",
        decision: "Fixture decision text",
        ...fields,
      });
    await call(400, "/decisions", "member", "POST", {
      projectId: project.id,
      title: "Unverified attribution",
      decidedOn: "2026-11-20",
      decision: "Example decision",
      madeByUserId: members.get("admin"),
    });
    await call(400, "/decisions", "captain", "POST", {
      projectId: project.id,
      title: "Incomplete crew relay",
      decidedOn: "2026-11-20",
      decision: "Example decision",
      madeByAgentRank: "crew",
    });
    const cache = await log("admin", {
      title: "Adopt the example cache",
      status: "accepted",
      tags: ["design"],
      issueIds: [issue.id],
    });
    const proposal = await log("captain", {
      title: "Agent proposal",
      decision: "Use the fixture queue",
      tags: ["design"],
    });
    await log("member", { title: "Rejected fixture", status: "rejected" });
    const old = await log("member", { title: "Earlier approach", status: "accepted" });
    await log("admin", { title: "Later approach", status: "accepted", supersedesId: old.id });

    const listed = await call<Page<Decision>>(200, "/decisions?project=CNT&limit=200");
    const byId = new Map(listed.items.map((item) => [item.id, item]));
    expect(byId.get(cache.id)).toMatchObject({
      madeBy: { userId: members.get("admin"), available: true, kind: "human" },
      recordedBy: { userId: members.get("admin"), available: true, kind: "human" },
    });
    expect(byId.get(cache.id)!.madeBy).toEqual(byId.get(cache.id)!.recordedBy);
    expect(byId.get(proposal.id)).toMatchObject({
      madeBy: { userId: members.get("captain"), available: true, kind: "agent" },
      madeByAgentName: agentInput("captain").name,
      recordedBy: { userId: members.get("captain"), available: true, kind: "agent" },
    });
    const detail = await call<Decision>(200, `/decisions/${cache.id}`);
    expect(detail.madeBy).toEqual(byId.get(cache.id)!.madeBy);
    expect(detail.recordedBy).toEqual(byId.get(cache.id)!.recordedBy);
    const revisions = await call<Page<{ id: string }>>(200, `/decisions/${cache.id}/revisions`);
    expect(
      await call(200, `/decisions/${cache.id}/revisions/${revisions.items[0]!.id}`),
    ).toMatchObject({ snapshot: { madeBy: detail.madeBy, recordedBy: detail.recordedBy } });
    const mcpList = await tool<Page<Decision>>("subspace_list_decisions", {
      filters: { project: "CNT", limit: 200 },
    });
    expect(mcpList.structuredContent.items.find((item) => item.id === proposal.id)).toMatchObject({
      madeBy: byId.get(proposal.id)!.madeBy,
      recordedBy: byId.get(proposal.id)!.recordedBy,
    });

    // Each count must equal what the list returns for the same filters.
    const expectCounts = async (filters: string, expected: Counts) => {
      const counts = await call<Counts>(200, `/decisions/counts?${filters}`);
      expect([filters, counts]).toEqual([filters, expected]);
      const items = (await call<Page<Decision>>(200, `/decisions?${filters}&limit=200`)).items;
      const tally = (status: string) => items.filter((item) => item.status === status).length;
      expect([filters, items.length, tally("proposed"), tally("accepted")]).toEqual([
        filters,
        expected.total,
        expected.proposed,
        expected.accepted,
      ]);
      expect([tally("rejected"), tally("superseded")]).toEqual([
        expected.rejected,
        expected.superseded,
      ]);
    };
    const none = { proposed: 0, accepted: 0, rejected: 0, superseded: 0, total: 0 };
    await expectCounts("project=CNT", {
      proposed: 1,
      accepted: 2,
      rejected: 1,
      superseded: 1,
      total: 5,
    });
    await expectCounts("project=cnt&madeByType=agent", { ...none, proposed: 1, total: 1 });
    await expectCounts("project=CNT&madeByType=human", {
      ...none,
      accepted: 2,
      rejected: 1,
      superseded: 1,
      total: 4,
    });
    await expectCounts("project=CNT&tag=design", { ...none, proposed: 1, accepted: 1, total: 2 });
    await expectCounts("project=CNT&tag=DESIGN", none);
    await expectCounts(`issue=${issue.identifier}`, { ...none, accepted: 1, total: 1 });
    await expectCounts("project=CNT&q=%20FIXTURE%20QUEUE%20", { ...none, proposed: 1, total: 1 });
    await expectCounts("project=CNT&q=approach", { ...none, accepted: 1, superseded: 1, total: 2 });
    expect(await call(200, "/decisions/counts?project=CNT&status=rejected&limit=1")).toEqual(
      await call(200, "/decisions/counts?project=CNT"),
    );
    expect(
      (await tool<Counts>("subspace_count_decisions", { filters: { project: "CNT" } }))
        .structuredContent,
    ).toEqual(await call(200, "/decisions/counts?project=CNT"));

    // Access and bad input fail exactly as the list does.
    for (const query of ["project=CNT", "project=NONE", `issue=${issue.id}`, "madeByType=robot"]) {
      for (const role of ["admin", "other"] as const) {
        const list = await http(`/decisions?${query}`, role);
        const counts = await http(`/decisions/counts?${query}`, role);
        expect([query, role, counts.status]).toEqual([query, role, list.status]);
        if (list.status !== 200) {
          const normalise = ({
            instance: _instance,
            correlationId: _correlationId,
            ...error
          }: Record<string, unknown>) => error;
          expect(normalise((await counts.json()) as Record<string, unknown>)).toEqual(
            normalise((await list.json()) as Record<string, unknown>),
          );
        } else await Promise.all([list.body?.cancel(), counts.body?.cancel()]);
      }
    }
    expect((await http("/decisions/counts")).status).toBe(401);
    expect(await call(200, "/decisions/counts", "other")).toEqual(none);

    // An agent that is deactivated stays named on its decisions, marked unavailable.
    const captain = members.get("captain")!;
    const agent = await call<Versioned>(200, `/agents/${captain}`);
    const disabled = await call<Versioned>(200, `/agents/${captain}/deactivate`, "admin", "POST", {
      version: agent.version,
    });
    try {
      const after = await call<Page<Decision>>(200, "/decisions?project=CNT&madeByType=agent");
      expect(after.items.map(({ recordedBy, madeBy }) => ({ recordedBy, madeBy }))).toEqual([
        {
          recordedBy: { ...byId.get(proposal.id)!.recordedBy, available: false },
          madeBy: { ...byId.get(proposal.id)!.madeBy!, available: false },
        },
      ]);
      expect(await call(200, `/decisions/${proposal.id}`)).toMatchObject({
        recordedBy: { userId: captain, available: false },
        madeBy: { userId: captain, available: false },
      });
    } finally {
      await call(200, `/agents/${captain}/reactivate`, "admin", "POST", {
        version: disabled.version,
      });
    }
  });

  test.skipIf(!webhookSecret || !fixtureOrg)(
    "GitHub merge closes linked issues once and streams the committed result",
    async () => {
      const closed = await createIssue("Closes fixture");
      const mentioned = await createIssue("Mention fixture");
      const project = await call<Versioned>(200, "/projects/PAR");
      const repository = `example/round-two-${suffix}`;
      await call(200, "/projects/PAR/repositories", "admin", "PUT", {
        version: project.version,
        repositories: [repository],
      });
      const text = `Closes: ${closed.identifier}\n\nSee ${mentioned.identifier}`;
      expect((await hook("pull_request", pull(repository, text, null), false)).status).toBe(401);
      expect((await hook("pull_request", pull(repository, text, null))).status).toBe(204);
      const stream = await http("/updates?project=PAR", "member");
      expect(stream.headers.get("content-type")).toContain("text/event-stream");
      const reader = stream.body!.getReader();
      async function until(matches: (event: string) => boolean) {
        const decoder = new TextDecoder();
        let pending = "";
        while (true) {
          const next = await reader.read();
          if (next.done) throw new Error("The update stream ended before the expected event.");
          pending += decoder.decode(next.value, { stream: true });
          const events = pending.split("\n\n");
          pending = events.pop() ?? "";
          const found = events.find(matches);
          if (found) return found;
        }
      }
      try {
        await until((event) => event.startsWith("event: ready\n"));
        const now = new Date().toISOString();
        const merged = pull(repository, text, now, 31, now);
        expect((await hook("pull_request", merged)).status).toBe(204);
        const update = await until(
          (event) =>
            event.startsWith("event: issueChanged") &&
            event.includes(closed.id) &&
            event.includes("issueUpdated"),
        );
        expect(update).toContain('"actorType":"integration"');
        expect(await call(200, `/issues/${closed.id}`)).toMatchObject({
          state: { type: "completed" },
          version: closed.version + 1,
        });
        expect(await call(200, `/issues/${mentioned.id}`)).toMatchObject({
          version: mentioned.version,
        });
        expect((await hook("pull_request", merged)).status).toBe(204);
        expect(await call(200, `/issues/${closed.id}/pull-requests`)).toMatchObject({
          items: [{ number: 31, state: "merged", completion: "completed" }],
        });
        expect(await call(200, `/issues/${closed.id}`)).toMatchObject({
          version: closed.version + 1,
        });
        await call(404, `/issues/${closed.id}/pull-requests`, "other");
        await call(404, "/updates?project=PAR", "other");
      } finally {
        await reader.cancel();
      }
    },
  );

  test.skipIf(!webhookSecret || !fixtureOrg)(
    "historical merges and closing references in code cannot complete issues",
    async () => {
      const historical = await createIssue("Historical merge fixture");
      const code = await createIssue("Code mention fixture");
      const repository = `example/historical-${suffix}`;
      const project = await call<Versioned>(200, "/projects/PAR");
      await call(200, "/projects/PAR/repositories", "admin", "PUT", {
        version: project.version,
        repositories: [repository],
      });
      const prior = new Date(Date.now() - 86400_000).toISOString();
      const now = new Date().toISOString();
      expect(
        (
          await hook(
            "pull_request",
            pull(repository, `Fixes ${historical.identifier}`, prior, 31, now),
          )
        ).status,
      ).toBe(204);
      expect(
        (
          await hook(
            "pull_request",
            pull(repository, "Fixes " + "`" + code.identifier + "`", now, 32, now),
          )
        ).status,
      ).toBe(204);
      expect(await call(200, `/issues/${historical.id}/pull-requests`)).toMatchObject({
        items: [{ state: "merged", completion: "none" }],
      });
      expect(await call(200, `/issues/${historical.id}`)).toMatchObject({
        version: historical.version,
      });
      expect(await call(200, `/issues/${code.id}/pull-requests`)).toMatchObject({ items: [] });
    },
  );

  test.skipIf(!webhookSecret || !fixtureOrg)(
    "signed deployment events are deduplicated and repository scoped",
    async () => {
      const repo = `example/deploy-${suffix}`;
      const project = await call<Versioned>(200, "/projects/PAR");
      await call(200, "/projects/PAR/repositories", "admin", "PUT", {
        version: project.version,
        repositories: [repo],
      });
      const event = {
        repository: { full_name: repo },
        deployment: { id: 45, sha: "a".repeat(40), environment: "example" },
        deployment_status: { id: 48, state: "success", created_at: new Date().toISOString() },
      };
      expect((await hook("deployment_status", event, false)).status).toBe(401);
      expect((await hook("deployment_status", event)).status).toBe(204);
      expect((await hook("deployment_status", event)).status).toBe(204);
      const page = await call<Page<{ environment: string; state: string }>>(
        200,
        "/projects/PAR/deployment-statuses",
        "member",
      );
      expect(page.items).toEqual([
        expect.objectContaining({ repository: repo, environment: "example", state: "success" }),
      ]);
      await call(404, "/projects/PAR/deployment-statuses", "other");
    },
  );

  test("planning updates project health and versioned milestones without losing assigned issues", async () => {
    const project = `/projects/${projectId}`;
    const updates = `${project}/updates`;
    const report = await call<{ update: Versioned }>(201, updates, "captain", "POST", {
      body: "On schedule",
      health: "onTrack",
    });
    await call(403, `${updates}/${report.update.id}`, "crew", "PATCH", {
      version: report.update.version,
      health: "atRisk",
    });
    const revised = await call<{ update: Versioned }>(
      200,
      `${updates}/${report.update.id}`,
      "captain",
      "PATCH",
      { version: report.update.version, health: "atRisk" },
    );
    expect(await call(200, project)).toMatchObject({ health: "atRisk" });
    expect(await call(200, `${updates}?includeContent=false`)).toMatchObject({
      items: [{ id: report.update.id, body: null, contentIncluded: false }],
    });
    await call(404, updates, "other");
    await call(204, `${updates}/${report.update.id}`, "admin", "DELETE", {
      version: revised.update.version,
    });
    expect(await call(200, project)).toMatchObject({ health: null });

    const milestones = `${project}/milestones`;
    const first = await call<Versioned>(201, milestones, "admin", "POST", {
      name: "Milestone one",
      targetDate: "2026-12-01",
    });
    const second = await call<Versioned>(201, milestones, "admin", "POST", {
      name: "Milestone two",
      targetDate: "2026-12-02",
    });
    const issue = await createIssue("Assigned milestone fixture", "admin", {
      milestoneId: second.id,
    });
    const moved = await call<Versioned>(200, `${milestones}/${second.id}`, "captain", "PATCH", {
      version: second.version,
      move: "up",
      completed: true,
    });
    await call(409, `${milestones}/${second.id}`, "admin", "DELETE", { version: second.version });
    const list = await call<Page<{ id: string; status: string }>>(200, milestones);
    expect(list.items.find(({ id }) => id === second.id)?.status).toBe("done");
    expect(list.items.findIndex(({ id }) => id === second.id)).toBeLessThan(
      list.items.findIndex(({ id }) => id === first.id),
    );
    await call(204, `${milestones}/${second.id}`, "admin", "DELETE", { version: moved.version });
    expect(await call(200, `/issues/${issue.id}`)).toMatchObject({
      milestoneId: null,
      version: issue.version + 1,
    });
  });

  test("labels preserve existing references when archived and cannot be edited across organisations", async () => {
    const label = await call<{ id: string }>(201, "/labels", "admin", "POST", {
      name: "Planning label",
      color: "#446688",
    });
    const issue = await createIssue("Label fixture", "admin", { labelIds: [label.id] });
    await call(404, `/labels/${label.id}`, "other", "PUT", {
      name: "Foreign change",
      color: "#446688",
    });
    expect(
      await call(200, `/labels/${label.id}`, "admin", "PUT", {
        name: "Planning label",
        color: "#446688",
        state: "archived",
      }),
    ).toMatchObject({ state: "archived" });
    expect(await call(200, `/issues/${issue.id}`)).toMatchObject({
      labels: [{ id: label.id, state: "archived" }],
    });
    expect(
      (await call<Page<{ id: string }>>(200, "/labels")).items.some(({ id }) => id === label.id),
    ).toBe(false);
  });

  test("desktop Markdown media references resolve and keep attached bytes available", async () => {
    const issue = await createIssue("Desktop image fixture");
    const form = new FormData();
    const gif = Buffer.from("R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7", "base64");
    form.append("file", new Blob([gif], { type: "image/gif" }), "example.gif");
    const uploaded = await fetch(`${origin}/api/v1/issues/${issue.id}/images`, {
      method: "POST",
      headers: { Authorization: `Bearer ${tokens.get("admin")}` },
      body: form,
    });
    expect(uploaded.status).toBe(201);
    const { id, url } = (await uploaded.json()) as { id: string; url: string };
    await call(404, `/issues/media/${id}`, "other");
    const path = `/issues/${issue.id}/comments`;
    const comment = await call<Versioned>(201, path, "admin", "POST", {
      body: `![Fixture](nett-media:${id})`,
    });
    expect(await call(200, `${path}/${comment.id}`, "member")).toMatchObject({
      media: [{ mediaId: id, kind: "image" }],
    });
    await call(409, `/issues/media/${id}/delete`, "admin", "POST", {});
    const otherIssue = await createIssue("Different issue fixture");
    await call(400, `/issues/${otherIssue.id}/comments`, "admin", "POST", {
      body: `![Wrong issue](nett-media:${id})`,
    });
    await call(200, `${path}/${comment.id}/delete`, "admin", "POST", {});
    const pathLink = await call<Versioned>(201, path, "admin", "POST", {
      body: `![API image](${url})`,
    });
    expect(await call(200, `${path}/${pathLink.id}`, "member")).toMatchObject({
      media: [{ mediaId: id, kind: "image" }],
    });
    await call(409, `/issues/media/${id}/delete`, "admin", "POST", {});
    await call(200, `${path}/${pathLink.id}/delete`, "admin", "POST", {});
    expect(await call(200, `/issues/media/${id}/delete`, "admin", "POST", {})).toBe(true);
  });

  test("desktop child pagination continues with the parent UUID and manual order", async () => {
    const parent = await createIssue("Desktop child paging fixture", "admin", { type: "tracker" });
    for (let index = 0; index < 51; index++) {
      await createIssue(`Child number ${index}`, "admin", { parentId: parent.id });
    }
    const detail = await call<{
      children: Page<Issue>;
      progress: { total: number; done: number; sample: { id: string }[]; hasMore: boolean };
    }>(200, `/issues/${parent.id}`);
    expect(detail.progress).toMatchObject({ total: 51, done: 0, hasMore: true });
    expect(detail.progress.sample).toHaveLength(40);
    expect(detail.children.items).toHaveLength(50);
    const cursor = encodeURIComponent(detail.children.nextCursor!);
    const continued = await call<Page<Issue>>(
      200,
      `/issues?parent=${parent.id}&order=manual&cursor=${cursor}`,
    );
    expect(continued.items).toHaveLength(1);
    expect(detail.children.items).not.toContainEqual(
      expect.objectContaining({ id: continued.items[0]!.id }),
    );
  }, 10_000);

  test("desktop activity windows filter out activity older than the cutoff", async () => {
    const issue = await createIssue("Desktop activity window fixture");
    const after = encodeURIComponent(new Date(Date.now() + 60_000).toISOString());
    const before = encodeURIComponent(new Date(Date.now() - 60_000).toISOString());
    expect(
      (
        await call<Page<Issue>>(
          200,
          `/issues?project=PAR&q=Desktop+activity+window&activitySince=${after}`,
        )
      ).items,
    ).toEqual([]);
    expect(
      await call(200, `/issues?project=PAR&q=Desktop+activity+window&activitySince=${before}`),
    ).toMatchObject({ items: [{ id: issue.id }] });
  });

  test("shared views resolve project filters and personal views stay private", async () => {
    const fixture = await createIssue("Saved view fixture", "member");
    const definition = { version: 2, tab: "all", projectId, q: "Saved view fixture" };
    const saved = await call<Versioned>(201, "/issue-views", "member", "POST", {
      name: "Example view",
      visibility: "personal",
      definition,
    });
    await call(404, `/issue-views/${saved.id}`, "admin");
    await call(400, "/issue-views", "other", "POST", {
      name: "Foreign view",
      visibility: "shared",
      definition,
    });
    const shared = await call<Versioned>(200, `/issue-views/${saved.id}`, "member", "PATCH", {
      version: saved.version,
      visibility: "shared",
    });
    await call(404, `/issue-views/${saved.id}`, "other");
    expect(await call(200, `/issue-views/${saved.id}`, "crew")).toMatchObject({
      definition,
      canDelete: false,
      canEdit: false,
    });
    expect(await call(200, "/issue-views", "captain")).toMatchObject({
      items: [{ id: shared.id, issueCount: { value: 1, atLeast: false } }],
    });
    await call(403, `/issue-views/${saved.id}`, "crew", "DELETE", { version: shared.version });
    await call(204, `/issue-views/${saved.id}`, "admin", "DELETE", { version: shared.version });
    expect(await call(200, `/issues/${fixture.id}`)).toMatchObject({ id: fixture.id });
  });

  test("workflow replacement validates state ownership and deleting a state remaps issues", async () => {
    const project = await call<Versioned>(200, "/projects/PAR");
    const path = "/projects/PAR/workflow-states";
    const rows = await call<Page<{ id: string; type: string; isDefault: boolean }>>(200, path);
    const added = { name: "Ready for approval", type: "started", isDefault: false };
    const input = { version: project.version, states: [...rows.items, added] };
    await call(403, path, "captain", "PUT", input);
    await call(400, path, "admin", "PUT", { ...input, states: [...rows.items.slice(1), added] });
    const saved = await call<Versioned>(200, path, "admin", "PUT", input);
    const list = await call<Page<{ id: string; type: string; name: string }>>(200, path);
    const extra = list.items.find(({ name }) => name === added.name)!;
    const done = list.items.find(({ type }) => type === "completed")!;
    const issue = await createIssue("Workflow removal fixture", "admin", { stateId: extra.id });
    await call(409, `${path}/${extra.id}?version=${saved.version}`, "admin", "DELETE");
    await call(
      200,
      `${path}/${extra.id}?version=${saved.version}&replacementStateId=${done.id}`,
      "admin",
      "DELETE",
    );
    expect(await call(200, `/issues/${issue.id}`)).toMatchObject({
      state: { type: "completed" },
      version: issue.version + 1,
    });
  });

  test("estimate-scale preview and update perform the same nearest-point mapping", async () => {
    const project = await call<Versioned>(200, "/projects/PAR");
    const path = "/projects/PAR/estimate-scale";
    const linear = await call<{ project: Versioned }>(200, path, "admin", "PUT", {
      version: project.version,
      scale: "linear",
    });
    await call(400, "/issues", "member", "POST", {
      projectId,
      title: "Invalid estimate",
      summary: "Fixture",
      estimate: 8,
    });
    const issue = await createIssue("Estimate mapping fixture", "admin", { estimate: 3 });
    expect(await call(200, `${path}/preview?scale=exponential`)).toMatchObject({
      mappings: [{ from: 3, to: 4, issues: 1 }],
    });
    expect(
      await call(200, path, "admin", "PUT", {
        version: linear.project.version,
        scale: "exponential",
      }),
    ).toMatchObject({ changes: { issuesMapped: 1 } });
    expect(await call(200, `/issues/${issue.id}`)).toMatchObject({
      estimate: 4,
      version: issue.version + 1,
    });
  });

  test("cycles schedule a current window and preserve outcomes when it closes", async () => {
    const project = await call<Versioned>(200, "/projects/PAR");
    const day = ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"][
      new Date().getUTCDay()
    ]!;
    const config = {
      version: project.version,
      enabled: true,
      lengthWeeks: 2,
      startDay: day,
      cyclesAhead: 1,
      carryOver: true,
    };
    await call(403, "/projects/PAR/cycle-settings", "captain", "PUT", config);
    const enabled = await call<{ project: Versioned }>(
      200,
      "/projects/PAR/cycle-settings",
      "admin",
      "PUT",
      config,
    );
    const cycles = await call<Page<{ id: string; state: string }>>(
      200,
      "/issue-cycles?project=PAR",
    );
    const current = cycles.items.find(({ state }) => state === "current")!;
    expect(cycles.items.map(({ state }) => state)).toEqual(["current", "upcoming"]);
    const path = `/issue-cycles/${current.id}`;
    const issue = await createIssue("Cycle assignment fixture");
    await call(404, path, "other");
    expect(
      await call(200, `${path}/issues`, "crew", "POST", { issues: [issue.identifier] }),
    ).toMatchObject({ added: 1, cycle: { progress: { counted: 1 } } });
    expect(await call(200, `${path}/issues`, "crew", "POST", { issues: [issue.id] })).toMatchObject(
      { added: 0 },
    );
    expect(
      await call(200, "/projects/PAR/cycle-settings", "admin", "PUT", {
        ...config,
        version: enabled.project.version,
        enabled: false,
      }),
    ).toMatchObject({ changes: { issuesReleased: 1, cyclesRemoved: 1 } });
    expect(await call(200, path)).toMatchObject({ state: "closed", finishedEarly: true });
    expect(await call(200, `${path}/results`)).toMatchObject({
      items: [{ id: issue.id, outcome: "notCompleted" }],
    });
    expect(await call(200, `/issues/${issue.id}`)).toMatchObject({ cycleId: null });
  });

  test("repository links add, resolve and remove without allowing foreign reverse lookups", async () => {
    const path = "/projects/PAR/repositories";
    const repository = `example/reverse-${suffix}`;
    const before = await call<{ version: number }>(200, path);
    const added = await call<{ version: number }>(200, path, "admin", "POST", {
      version: before.version,
      repository,
    });
    const reverse = `/repositories/projects?repository=${encodeURIComponent(repository)}`;
    expect(await call(200, reverse, "member")).toMatchObject({ items: [{ id: projectId }] });
    expect(await call(200, reverse, "other")).toMatchObject({ items: [] });
    await call(409, path, "admin", "DELETE", { version: before.version, repository });
    await call(200, path, "admin", "DELETE", { version: added.version, repository });
    expect(await call(200, reverse)).toMatchObject({ items: [] });
  });

  test("foreign list endpoints omit organisation records even when their filters are unscoped", async () => {
    const issue = await createIssue("Organisation boundary fixture");
    const label = await call<Versioned>(201, "/labels", "admin", "POST", {
      name: "Organisation boundary",
      color: "#046f75",
    });
    const privateRecords = [
      ["/users", members.get("admin")],
      ["/agents", members.get("captain")],
      ["/initiatives", initiativeId],
      ["/fleets", fleetId],
      ["/projects", projectId],
      ["/labels", label.id],
      ["/issues", issue.id],
    ] as const;
    for (const [path, id] of privateRecords) {
      type Entry = { id?: string; userId?: string };
      const isFixture = (row: Entry) => (row.id ?? row.userId) === id;
      const own = await call<Page<Entry>>(200, path);
      expect(own.items.some(isFixture)).toBeTrue();
      const foreign = await call<Page<Entry>>(200, path, "other");
      expect(foreign.items.some(isFixture)).toBeFalse();
    }
    expect((await call<Page<unknown>>(200, "/notifications", "other")).items).toEqual([]);
    for (const child of [
      "activity",
      "attachments",
      "comments",
      "documents",
      "images",
      "relations",
      "reviews",
      "subscribers",
      "todos",
    ]) {
      await call(404, `/issues/${issue.id}/${child}`, "other");
    }
    const comment = await call<Versioned>(201, `/issues/${issue.id}/comments`, "admin", "POST", {
      body: "Private comment",
    });
    await call(404, `/issue-comments/${comment.id}/location`, "other");
    await call(404, `/initiatives/${initiativeId}`, "other");
    await call(404, `/initiatives/${initiativeId}/projects`, "other");
  });

  test.each([
    "/projects/PAR",
    "/projects/PAR/workflow-states",
    "/projects/PAR/milestones",
    "/projects/PAR/repositories",
    "/projects/PAR/review-requirements",
    "/issue-cycles?project=PAR",
    "/issues?project=PAR",
    "/issues/groups?project=PAR&groupBy=state",
    "/audit?project=PAR",
  ])("foreign organisation has no access to %s", async (path) => {
    await call(404, path, "other");
  });
});
