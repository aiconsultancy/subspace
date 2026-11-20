import { beforeAll, describe, expect, test } from "bun:test";
import { createHmac } from "node:crypto";

const baseUrl = process.env.TEST_API_URL?.replace(/\/$/u, "");
const version = "/api/v1";
const webhookSecret = process.env.TEST_GITHUB_WEBHOOK_SECRET;
const fixtureOrg = process.env.TEST_GITHUB_ORGANISATION;
const runId = crypto.randomUUID().slice(0, 8);
let admin: string;
let ordinary: string;
let other: string;
let adminId: string;
let ordinaryId: string;
let initiativeId: string;
let projectId: string;
let defaultStateId: string;
let key: string;

async function request(path: string, options: RequestInit = {}, token?: string) {
  const headers = new Headers(options.headers);
  if (token) headers.set("Authorization", `Bearer ${token}`);
  if (typeof options.body === "string") headers.set("Content-Type", "application/json");
  return fetch(`${baseUrl}${version}${path}`, {
    ...options,
    headers,
    signal: options.signal ?? AbortSignal.timeout(5000),
  });
}

async function json<T>(
  status: number,
  path: string,
  token?: string,
  method = "GET",
  data?: unknown,
): Promise<T> {
  const response = await request(
    path,
    { method, body: data === undefined ? undefined : JSON.stringify(data) },
    token,
  );
  expect(response.status).toBe(status);
  return response.json() as Promise<T>;
}

async function signIn(email: string, organisation: string) {
  const result = await json<{ accessToken: string; tokenType: string; expiresIn: number }>(
    200,
    "/auth/dev/token",
    undefined,
    "POST",
    { email, organisation },
  );
  expect(result.tokenType).toBe("Bearer");
  expect(result.expiresIn).toBe(28_800);
  return result.accessToken;
}

type Item = { id: string; name: string; version: number };
type Page<T> = { items: T[]; nextCursor: string | null };
type Fleet = Item & {
  slug: string;
  irc: { host: string; port: number; tls: boolean; websocketUrl: string | null };
};
type Issue = Item & { identifier: string; title: string; state: { id: string; type: string } };
type Comment = Item & {
  body: string;
  actualAuthor: { userId: string };
  parentCommentId: string | null;
};

describe.skipIf(!baseUrl)("desktop API against Compose PostgreSQL", () => {
  beforeAll(async () => {
    const organisation = `verify-${runId}`;
    admin = await signIn("admin@example.com", organisation);
    ordinary = await signIn("member@example.com", organisation);
    other = await signIn("operator@example.com", `separate-${runId}`);
    const me = await json<{ userId: string; kind: string }>(200, "/me", admin);
    adminId = me.userId;
    ordinaryId = (await json<{ userId: string }>(200, "/me", ordinary)).userId;
    expect(me.kind).toBe("human");
    const mine = await json<Page<Item>>(200, "/me/initiatives", admin);
    initiativeId = mine.items[0]!.id;
    key = "DEMO";
    const project = await json<{ id: string; defaultStateId: string }>(
      201,
      "/projects",
      admin,
      "POST",
      { key, name: "Example project" },
    );
    projectId = project.id;
    defaultStateId = project.defaultStateId;
  });

  test("health, authentication and membership", async () => {
    const health = await fetch(`${baseUrl}/health`);
    expect(health.status).toBe(200);
    expect(await health.json()).toEqual({ status: "ok" });
    const refusal = await request("/me");
    expect(refusal.status).toBe(401);
    expect(refusal.headers.get("content-type")).toContain("application/problem+json");
    expect((await request("/me", {}, "invalid")).status).toBe(401);
    const crossSite = await request("/auth/dev/token", {
      method: "POST",
      body: "{}",
      headers: { Origin: "https://example.com" },
    });
    expect(crossSite.status).toBe(403);
    const local = await request("/auth/dev/token", {
      method: "POST",
      body: JSON.stringify({ organisation: `host-fixture-${runId}` }),
      headers: { Host: "example.com" },
    });
    expect(local.status).toBe(200);
    await local.body?.cancel();
    expect((await json<Page<Item>>(200, "/me/initiatives", ordinary)).items).toEqual([]);
    const current = await request("/me", {}, admin);
    expect(current.headers.get("cache-control")).toBe("private, no-store");
  });

  test("fleet registration, amendment, assignment and organisation isolation", async () => {
    const data = {
      slug: "example",
      name: "Example Fleet",
      ircHost: "irc.example.com",
      ircPort: 6697,
      ircTls: true,
    };
    expect(
      (await request("/fleets", { method: "POST", body: JSON.stringify(data) }, ordinary)).status,
    ).toBe(403);
    const fleet = await json<Fleet>(201, "/fleets", admin, "POST", data);
    expect(fleet.irc).toEqual({
      host: "irc.example.com",
      port: 6697,
      tls: true,
      websocketUrl: null,
    });
    expect(
      (await json<Page<Fleet>>(200, "/fleets", ordinary)).items.some(({ id }) => id === fleet.id),
    ).toBe(true);
    expect((await json<Fleet>(200, "/fleets/example", admin)).id).toBe(fleet.id);
    expect(
      (await json<Fleet>(200, `/fleets/${fleet.id}`, admin, "PATCH", { vmHost: "vm.example.com" }))
        .id,
    ).toBe(fleet.id);
    await json(409, "/fleets", admin, "POST", data);
    await json(404, `/fleets/${fleet.id}`, other);
    await json(404, `/initiatives/${initiativeId}/fleet`, other, "PUT", { fleetId: fleet.id });
    const foreign = await json<Page<Fleet>>(200, "/fleets", other);
    await json(404, `/initiatives/${initiativeId}/fleet`, admin, "PUT", {
      fleetId: foreign.items[0]!.id,
    });
    const result = await json<{ fleetId: string }>(
      200,
      `/initiatives/${initiativeId}/fleet`,
      admin,
      "PUT",
      { fleetId: fleet.id },
    );
    expect(result.fleetId).toBe(fleet.id);
    const mine = await json<Page<{ fleet: Fleet }>>(200, "/me/initiatives", admin);
    expect(mine.items[0]?.fleet.id).toBe(fleet.id);
    const unassigned = await json<{ fleetId: null }>(
      200,
      `/initiatives/${initiativeId}/fleet`,
      admin,
      "PUT",
      { fleetId: null },
    );
    expect(unassigned.fleetId).toBeNull();
  });

  test("initiative channel membership, optimistic conflicts and archived branches", async () => {
    const parent = await json<Item>(201, "/initiatives", admin, "POST", {
      name: "Parent initiative",
      ownerId: adminId,
    });
    const child = await json<Item>(201, "/initiatives", admin, "POST", {
      name: "Child initiative",
      ownerId: adminId,
      parentId: parent.id,
    });
    await json(403, `/initiatives/${child.id}/channel-configuration`, ordinary, "PATCH", {
      version: child.version,
      channels: "#example",
    });
    const saved = await json<{ channels: string; version: number; memberUserIds: string[] }>(
      200,
      `/initiatives/${child.id}/channel-configuration`,
      admin,
      "PATCH",
      { version: child.version, channels: "#example, #review", addMemberUserIds: [ordinaryId] },
    );
    expect(saved.channels).toBe("#example, #review");
    expect(saved.memberUserIds).toEqual([ordinaryId]);
    const conflict = await json<{ currentVersion: number }>(
      409,
      `/initiatives/${child.id}/channel-configuration`,
      admin,
      "PATCH",
      { version: child.version, channels: "#example" },
    );
    expect(conflict.currentVersion).toBe(saved.version);
    const selector = await json<Page<Item>>(200, "/me/initiatives", ordinary);
    expect(selector.items[0]?.id).toBe(child.id);
    await json(200, `/initiatives/${parent.id}/archive`, admin, "POST", {
      version: parent.version,
    });
    expect((await json<Page<Item>>(200, "/me/initiatives", ordinary)).items).toEqual([]);
    const archivedBranch = await json<Page<Item>>(
      200,
      `/initiatives?rootId=${parent.id}&includeArchived=true`,
      admin,
    );
    expect(archivedBranch.items.map(({ id }) => id)).toEqual([child.id]);
  });

  test("project, member, labels, milestone and cycle catalogues", async () => {
    const projects = await json<Page<Item>>(200, "/projects?includeArchived=true&limit=200", admin);
    expect(projects.items.map(({ id }) => id)).toEqual([projectId]);
    const project = await json<{
      id: string;
      initiativeId: string | null;
      version: number;
      estimateScale: string;
    }>(200, `/projects/${key}`, admin);
    expect(project.estimateScale).toBe("notUsed");
    await json(200, `/projects/${key}/initiative`, admin, "PUT", {
      version: project.version,
      initiativeId,
    });
    const initiativeProjects = await json<Page<Item>>(
      200,
      `/initiatives/${initiativeId}/projects?scope=direct`,
      admin,
    );
    expect(initiativeProjects.items.map(({ id }) => id)).toEqual([projectId]);
    const states = await json<Page<{ id: string; type: string }>>(
      200,
      `/projects/${key}/workflow-states`,
      admin,
    );
    expect(states.items.map(({ type }) => type)).toEqual([
      "triage",
      "backlog",
      "unstarted",
      "started",
      "completed",
      "canceled",
      "duplicate",
    ]);
    expect(states.items.some(({ id }) => id === defaultStateId)).toBeTrue();
    const users = await json<Page<Item>>(200, "/users", admin);
    expect(users.items.map(({ id }) => id).sort()).toEqual([adminId, ordinaryId].sort());
    const label = await json<Item>(201, "/labels", admin, "POST", {
      name: "Example label",
      color: "#374151",
    });
    expect((await json<Page<Item>>(200, "/labels?includeArchived=false", admin)).items[0]?.id).toBe(
      label.id,
    );
    const milestone = await json<Item>(201, `/projects/${key}/milestones`, admin, "POST", {
      name: "First milestone",
    });
    expect(
      (
        await json<Page<Item>>(
          200,
          `/projects/${key}/milestones?milestoneId=${milestone.id}&includeContent=false`,
          admin,
        )
      ).items[0]?.id,
    ).toBe(milestone.id);
    expect(await json<Page<Item>>(200, `/issue-cycles?project=${key}`, admin)).toEqual({
      items: [],
      nextCursor: null,
    });
  });

  function createIssue(title: string) {
    return json<Issue>(201, "/issues", admin, "POST", {
      projectId,
      title,
      summary: "Integration test issue",
    });
  }

  test("grouped list, continuation, issue detail and versioned sparse writes", async () => {
    const first = await createIssue("Grouped issue A");
    await createIssue("Grouped issue B");
    const qs = `project=${key}&q=Grouped&order=priority`;
    const listing = await json<{
      groups: { stateId: string; count: number; nextCursor: string; items: Issue[] }[];
    }>(200, `/issues/groups?groupBy=state&limitPerGroup=1&${qs}`, admin);
    const backlog = listing.groups.find(({ stateId }) => stateId === defaultStateId)!;
    expect(backlog.count).toBe(2);
    expect(backlog.items).toHaveLength(1);
    const next = await json<Page<Issue>>(
      200,
      `/issues?${qs}&stateId=${defaultStateId}&cursor=${backlog.nextCursor}`,
      admin,
    );
    expect(next.items).toHaveLength(1);
    expect(next.items[0]?.id).not.toBe(backlog.items[0]?.id);
    await json(400, `/issues?project=${key}&q=other&cursor=${backlog.nextCursor}`, admin);

    const read = await json<Issue>(200, `/issues/${first.identifier}`, ordinary);
    expect(read.state.type).toBe("backlog");
    const updated = await json<Issue>(200, `/issues/${first.id}`, admin, "PATCH", {
      version: first.version,
      title: "Edited title",
    });
    expect(updated.title).toBe("Edited title");
    expect(updated.version).toBe(first.version + 1);
    const stale = await json<{ currentVersion: number }>(
      409,
      `/issues/${first.id}`,
      admin,
      "PATCH",
      { version: first.version, title: "Unapplied edit" },
    );
    expect(stale.currentVersion).toBe(updated.version);
    await json(404, `/issues/${first.id}`, other);
  });

  test("issue SQL continuations preserve cursor order across all four sort modes", async () => {
    type Row = Issue & { priority: number; createdAt: string; lastActivityAt: string };
    const search = "Cursor fixture %_";
    const fixtures: Row[] = [];
    for (const priority of [2, 1, 0, 1]) {
      fixtures.push(
        await json<Row>(201, "/issues", admin, "POST", {
          projectId,
          title: `${search} ${fixtures.length}`,
          summary: "Pagination fixture",
          priority,
        }),
      );
    }
    await createIssue("Cursor fixture should not match the literal punctuation");
    await json(201, `/issues/${fixtures[0]!.id}/comments`, admin, "POST", {
      body: "Refresh activity order",
    });
    fixtures[0] = await json<Row>(200, `/issues/${fixtures[0]!.id}`, admin);

    const created = (a: Row, b: Row) =>
      Date.parse(b.createdAt) - Date.parse(a.createdAt) || a.id.localeCompare(b.id);
    const millisecondsKey = (time: string) =>
      String(Number.MAX_SAFE_INTEGER - Date.parse(time)).padStart(16, "0");
    const numeric = ({ identifier }: Row) => Number(identifier.split("-").at(-1));
    const orders = {
      created,
      priority: (a: Row, b: Row) => (a.priority || 5) - (b.priority || 5) || created(a, b),
      activity: (a: Row, b: Row) =>
        Date.parse(b.lastActivityAt) - Date.parse(a.lastActivityAt) || created(a, b),
      manual: (a: Row, b: Row) => numeric(a) - numeric(b),
    };
    for (const order of Object.keys(orders) as (keyof typeof orders)[]) {
      const query = `/issues?project=${key}&q=${encodeURIComponent(search)}&order=${order}&limit=1`;
      const expected = fixtures.toSorted(orders[order]);
      let cursor: string | null = null;
      for (const [index, row] of expected.entries()) {
        const page: Page<Row> = await json(200, query + (cursor ? `&cursor=${cursor}` : ""), admin);
        expect(page.items.map(({ id }) => id)).toEqual([row.id]);
        cursor = page.nextCursor;
        if (index === expected.length - 1) {
          expect(cursor).toBeNull();
          continue;
        }
        const tie = `${millisecondsKey(row.createdAt)}:${row.id}`;
        const expectedAnchor =
          order === "activity"
            ? `${millisecondsKey(row.lastActivityAt)}:${tie}`
            : order === "priority"
              ? `${row.priority || 5}:${tie}`
              : order === "manual"
                ? `${String(numeric(row)).padStart(12, "0")}:${row.id}`
                : tie;
        const [, anchor] = JSON.parse(Buffer.from(cursor!, "base64url").toString("utf8"));
        expect(anchor).toBe(expectedAnchor);
      }
    }
  });

  test("comment threads, issue activity and live commit notification", async () => {
    const issue = await createIssue("Comment activity");
    const streaming = await request(`/updates?project=${key}`, {}, ordinary);
    expect(streaming.status).toBe(200);
    expect(streaming.headers.get("content-type")).toBe("text/event-stream");
    const reader = streaming.body!.getReader();
    const decoder = new TextDecoder();
    try {
      const ready = await reader.read();
      expect(decoder.decode(ready.value)).toContain("event: ready");
      const endpoint = `/issues/${issue.identifier}/comments`;
      const root = await json<Comment>(201, endpoint, admin, "POST", { body: "Root comment" });
      const firstChunk = await reader.read();
      expect(decoder.decode(firstChunk.value)).toContain("event: issueChanged");
      const second = await json<Comment>(201, endpoint, ordinary, "POST", {
        body: "Reply",
        parentCommentId: root.id,
      });
      const roots = await json<Page<Comment>>(200, endpoint, admin);
      expect(roots.items[0]?.actualAuthor.userId).toBe(adminId);
      expect(roots.items).toHaveLength(1);
      const replies = await json<Page<Comment>>(200, `${endpoint}/${root.id}/replies`, admin);
      expect(replies.items.map(({ id }) => id)).toEqual([second.id]);
      const activity = await json<Page<{ kind: string }>>(
        200,
        `/issues/${issue.id}/activity`,
        admin,
      );
      expect(activity.items.map(({ kind }) => kind).sort()).toEqual([
        "comment.created",
        "comment.created",
        "issue.created",
      ]);
      await json(404, `/updates?project=${key}`, other);
    } finally {
      await reader.cancel();
    }
  });

  test("streams outnumber the SQL pool and remain scoped to the member and project", async () => {
    const poolSize = Number(process.env.TEST_DATABASE_POOL_SIZE ?? 10);
    const perUser = Number(process.env.TEST_SSE_MAX_STREAMS_PER_USER ?? 16);
    const maxStreams = Number(process.env.TEST_SSE_MAX_STREAMS ?? 256);
    const decode = new TextDecoder();
    const readers: ReadableStreamDefaultReader<Uint8Array>[] = [];
    async function connect(token: string) {
      const response = await request(`/updates?project=${key}`, {}, token);
      expect(response.status).toBe(200);
      const reader = response.body!.getReader();
      readers.push(reader);
      expect(decode.decode((await reader.read()).value)).toContain("event: ready");
      return reader;
    }
    const foreign = await json<Item>(201, "/projects", other, "POST", {
      key,
      name: "Foreign project",
    });
    const unobservedKey = "ELSE";
    const differentProject = await json<Item>(201, "/projects", admin, "POST", {
      key: unobservedKey,
      name: "Unobserved project",
    });
    const streamCount = Math.max(poolSize + 1, perUser);
    try {
      for (let index = 0; index < streamCount; index++) await connect(admin);
      const foreignStream = await connect(other);
      const elapsedFrom = performance.now();
      const health = await fetch(`${baseUrl}/health`, { signal: AbortSignal.timeout(1000) });
      expect(health.status).toBe(200);
      expect(await health.json()).toEqual({ status: "ok" });
      expect(performance.now() - elapsedFrom).toBeLessThan(1000);
      await json(200, `/issues?project=${key}&limit=1`, admin);
      expect((await request(`/updates?project=${key}`, {}, admin)).status).toBe(429);
      if (maxStreams === streamCount + 2) {
        const secondUser = await connect(ordinary);
        expect((await request(`/updates?project=${key}`, {}, ordinary)).status).toBe(429);
        await secondUser.cancel();
      }

      async function makeIssue(token: string, id: string, title: string) {
        return json<Issue>(201, "/issues", token, "POST", { projectId: id, title, summary: title });
      }
      await makeIssue(admin, differentProject.id, "Unobserved event");
      const scoped = await makeIssue(admin, projectId, "Local fan-out");
      const isolated = await makeIssue(other, foreign.id, "Foreign fan-out");
      for (const reader of readers.slice(0, streamCount)) {
        const event = decode.decode((await reader.read()).value);
        expect(event).toContain("event: issueChanged");
        expect(event).toContain(scoped.id);
      }
      expect(decode.decode((await foreignStream.read()).value)).toContain(isolated.id);
    } finally {
      await Promise.all(readers.map((reader) => reader.cancel()));
    }
    try {
      await connect(admin);
    } finally {
      await readers.at(-1)?.cancel();
    }
  });

  test("chat upload idempotency, content, resolution and access rules", async () => {
    const contents = "Example chat file";
    const data = {
      uploadId: crypto.randomUUID(),
      fileName: "example.txt",
      contentType: "text/plain",
      base64: Buffer.from(contents).toString("base64"),
    };
    const entry = await json<{ id: string; reference: string; expiresAt: string }>(
      201,
      "/chat/uploads",
      admin,
      "POST",
      data,
    );
    expect(entry.id).toBe(data.uploadId);
    const retry = await json<typeof entry>(201, "/chat/uploads", admin, "POST", data);
    expect(retry).toEqual(entry);
    const route = `/chat/uploads/${entry.reference}`;
    expect(await json<typeof entry>(200, route, ordinary)).toEqual(entry);
    await json(404, route, other);
    await json(403, route, ordinary, "DELETE");
    const resolved = await json<Page<{ status: string }>>(200, "/resolve", ordinary, "POST", {
      references: [entry.reference, "missing"],
    });
    expect(resolved.items.map(({ status }) => status)).toEqual(["available", "missing"]);
    const content = await request(`${route}/content`, {}, ordinary);
    expect(content.headers.get("x-content-type-options")).toBe("nosniff");
    expect(await content.text()).toBe(contents);
    expect(await json<boolean>(200, route, admin, "DELETE")).toBe(true);
    await json(404, route, admin);
  });

  test("multipart attachments and image reads and deletes", async () => {
    const issue = await createIssue("File issue");
    const prefix = `/issues/${issue.id}`;
    const upload = async (name: string, body: Blob, path: string) => {
      const form = new FormData();
      form.append("file", body, name);
      const response = await request(path, { method: "POST", body: form }, admin);
      expect(response.status).toBe(201);
      return response.json() as Promise<{ id?: string; mediaId?: string; url: string }>;
    };
    const txt = await upload(
      "example.txt",
      new Blob(["example"], { type: "text/plain" }),
      `${prefix}/attachments`,
    );
    expect((await json<Page<Item>>(200, `${prefix}/attachments`, admin)).items).toHaveLength(1);
    const content = await request(`/issues/attachments/${txt.mediaId}`, {}, ordinary);
    expect(await content.text()).toBe("example");
    await json(404, `/issues/attachments/${txt.mediaId}`, other);
    expect(
      await json<boolean>(200, `/issues/attachments/${txt.mediaId}`, admin, "DELETE", {}),
    ).toBe(true);
    const gif = Buffer.from("R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7", "base64");
    const image = await upload(
      "example.gif",
      new Blob([gif], { type: "image/gif" }),
      `${prefix}/images`,
    );
    expect((await json<Page<Item>>(200, `${prefix}/images`, admin)).items[0]?.id).toBe(image.id);
    const bytes = await request(`/issues/media/${image.id}`, {}, admin);
    expect(Buffer.from(await bytes.arrayBuffer())).toEqual(gif);
    expect(await json<boolean>(200, `/issues/media/${image.id}/delete`, admin, "POST", {})).toBe(
      true,
    );
  });
  test("issue relations and move previews preserve UUID and old identifier", async () => {
    const root = await createIssue("Moving root");
    const child = await json<Issue>(201, "/issues", admin, "POST", {
      projectId,
      title: "Moving child",
      summary: "Child fixture",
      parentId: root.id,
    });
    const related = await createIssue("Related issue");
    const updated = await json<Issue>(200, `/issues/${root.id}`, admin, "PATCH", {
      version: root.version,
      relations: { add: { blocks: [related.id] } },
    });
    const links = await json<Page<{ id: string; kind: string }>>(
      200,
      `/issues/${related.id}/relations`,
      admin,
    );
    expect(links.items.map(({ id, kind }) => ({ id, kind }))).toEqual([
      { id: root.id, kind: "blockedBy" },
    ]);
    const destination = await json<Item>(201, "/projects", admin, "POST", {
      key: "NEXT",
      name: "Destination project",
    });
    const args = { destinationProjectId: destination.id, version: updated.version };
    const previewPath = `/issues/${root.id}/move-preview`;
    const movePath = `/issues/${root.id}/move`;
    const preview = await json<{
      token: string;
      issues: Array<{ id: string; newIdentifier: string }>;
    }>(200, previewPath, admin, "POST", args);
    expect(preview.token).toMatch(/^[0-9a-f]{64}$/);
    expect(preview.issues.map(({ id }) => id)).toEqual([root.id, child.id]);
    await json<Issue>(201, "/issues", admin, "POST", {
      projectId: destination.id,
      title: "Another issue",
      summary: "Consumes the preview identifier",
    });
    await json(409, movePath, admin, "POST", { ...args, token: preview.token });
    const fresh = await json<typeof preview>(200, previewPath, admin, "POST", args);
    const result = await json<{ issue: Issue; moved: typeof preview.issues }>(
      200,
      movePath,
      admin,
      "POST",
      { ...args, token: fresh.token },
    );
    expect(result.issue.id).toBe(root.id);
    expect(result.issue.identifier).toBe("NEXT-2");
    expect(result.moved[1]?.newIdentifier).toBe("NEXT-3");
    expect((await json<Issue>(200, `/issues/${root.identifier}`, admin)).identifier).toBe("NEXT-2");
  });

  test("decision detail and filtered list include linked issue and timeline references", async () => {
    const issue = await createIssue("Decision issue");
    const endpoint = "/decisions";
    const data = {
      projectId,
      issueIds: [issue.id],
      title: "Example decision",
      decidedOn: "2026-01-10",
      decision: "Record the example",
      status: "accepted",
      tags: ["design"],
    };
    const decision = await json<{
      id: string;
      status: string;
      madeByType: string;
      recordedBy: { userId: string };
    }>(201, endpoint, admin, "POST", data);
    expect(decision.status).toBe("accepted");
    expect(decision.madeByType).toBe("human");
    expect(decision.recordedBy.userId).toBe(adminId);
    const list = await json<Page<{ id: string }>>(
      200,
      `${endpoint}?issue=${issue.identifier}&tag=design&status=proposed&status=accepted`,
      admin,
    );
    expect(list.items.map(({ id }) => id)).toEqual([decision.id]);
    expect(await json(200, `${endpoint}/${decision.id}`, ordinary)).toEqual(decision);
    await json(404, `${endpoint}/${decision.id}`, other);
    const activity = await json<
      Page<{ kind: string; displayAt: string }> & {
        references: { decisions: Array<{ id: string }> };
      }
    >(200, `/issues/${issue.id}/activity`, admin);
    expect(
      activity.items.some(
        ({ kind, displayAt }) => kind === "decision.logged" && displayAt.length > 0,
      ),
    ).toBe(true);
    expect(activity.references.decisions[0]?.id).toBe(decision.id);
  });

  test.skipIf(!webhookSecret || !fixtureOrg)(
    "signed GitHub pull-request events link only mapped project issues",
    async () => {
      const githubAdmin = await signIn("admin@example.com", fixtureOrg!);
      const githubMember = await signIn("member@example.com", fixtureOrg!);
      const project = await json<Item>(201, "/projects", githubAdmin, "POST", {
        key: "GIT",
        name: "Webhook fixture",
      });
      const issue = await json<Issue>(201, "/issues", githubAdmin, "POST", {
        projectId: project.id,
        title: "Pull request issue",
        summary: "Webhook fixture",
      });
      const repository = `example/workspace-${runId}`;
      const repos = await json<Page<{ repository: string }>>(
        200,
        `/projects/${project.id}/repositories`,
        githubAdmin,
        "PUT",
        {
          version: project.version,
          repositories: [repository],
        },
      );
      expect(repos.items[0]?.repository).toBe(repository);
      const foreign = await json<Item>(201, "/projects", other, "POST", {
        key: "DUP",
        name: "Foreign webhook fixture",
      });
      await json(403, `/projects/${foreign.id}/repositories`, other, "PUT", {
        version: foreign.version,
        repositories: [repository],
      });
      expect(await json(200, `/projects/${project.id}/repositories`, githubAdmin)).toEqual(repos);
      const body = JSON.stringify({
        repository: { full_name: repository },
        pull_request: {
          number: 10,
          title: "Example change",
          body: `Fixes ${issue.identifier}`,
          user: { login: "example" },
          head: { ref: "change" },
          base: { ref: "main" },
          state: "open",
          draft: false,
          merged_at: null,
          closed_at: null,
          updated_at: new Date().toISOString(),
        },
      });
      const hooks = { method: "POST", body, headers: { "x-github-event": "pull_request" } };
      expect((await request("/integrations/github/webhook", hooks)).status).toBe(401);
      const signature = createHmac("sha256", webhookSecret!).update(body).digest("hex");
      const delivered = await request("/integrations/github/webhook", {
        ...hooks,
        headers: { ...hooks.headers, "x-hub-signature-256": `sha256=${signature}` },
      });
      expect(delivered.status).toBe(204);
      const pulls = await json<Page<{ number: number; referenceKind: string }>>(
        200,
        `/issues/${issue.id}/pull-requests`,
        githubMember,
      );
      expect(pulls.items[0]).toMatchObject({ number: 10, referenceKind: "closes" });
      await json(404, `/issues/${issue.id}/pull-requests`, other);
      const detail = await json<{ pullRequestSummary: { total: number } }>(
        200,
        `/issues/${issue.id}`,
        githubMember,
      );
      expect(detail.pullRequestSummary.total).toBe(1);
    },
  );
  test("tracker states follow children and issue archive retains attribution", async () => {
    const tracker = await json<Issue>(201, "/issues", admin, "POST", {
      projectId,
      type: "tracker",
      title: "Issue tracker",
      summary: "Tracks the fixture child",
    });
    const states = await json<Page<{ id: string; type: string }>>(
      200,
      `/projects/${key}/workflow-states`,
      admin,
    );
    const completed = states.items.find(({ type }) => type === "completed")!;
    const child = await json<Issue>(201, "/issues", admin, "POST", {
      projectId,
      title: "Completed child",
      summary: "Completed child fixture",
      parentId: tracker.id,
      stateId: completed.id,
    });
    expect((await json<Issue>(200, `/issues/${tracker.id}`, admin)).state.type).toBe("completed");
    const archive = await json<Issue & { archivedBy: { userId: string } }>(
      200,
      `/issues/${child.id}/archive`,
      admin,
      "POST",
      { version: child.version },
    );
    expect(archive.archivedBy.userId).toBe(adminId);
    expect((await json<Issue>(200, `/issues/${tracker.id}`, admin)).state.type).toBe("backlog");
    await json(200, `/issues/${child.id}/unarchive`, admin, "POST", { version: archive.version });
    expect((await json<Issue>(200, `/issues/${tracker.id}`, admin)).state.type).toBe("completed");
  });

  test("todo batch, versioned changes, reorder, deletion and Issue detail", async () => {
    const issue = await createIssue("Issue with todos");
    type Todo = {
      id: string;
      text: string;
      version: number;
      done: boolean;
      doneBy: { userId: string } | null;
    };
    const path = `/issues/${issue.id}/todos`;
    const created = await json<Page<Todo>>(201, path, admin, "POST", {
      items: [{ text: "First item" }, { text: "Second item" }],
    });
    const [first, second] = created.items;
    const saved = await json<Todo>(200, `${path}/${first!.id}`, ordinary, "PATCH", {
      version: first!.version,
      done: true,
    });
    expect(saved.done).toBe(true);
    expect(saved.doneBy?.userId).toBe(ordinaryId);
    await json(409, `${path}/${first!.id}`, ordinary, "PATCH", {
      version: first!.version,
      text: "Stale change",
    });
    const ordered = await json<Page<Todo>>(200, `${path}/order`, admin, "PUT", {
      todoIds: [second!.id, first!.id],
    });
    expect(ordered.items.map(({ id }) => id)).toEqual([second!.id, first!.id]);
    const detail = await json<{ todos: Todo[]; todoCounts: { total: number; done: number } }>(
      200,
      `/issues/${issue.id}`,
      admin,
    );
    expect(detail.todos).toEqual(ordered.items);
    expect(detail.todoCounts).toEqual({ total: 2, done: 1 });
    await json(200, `${path}/${second!.id}/delete`, admin, "POST", {
      version: ordered.items[0]!.version,
    });
    expect((await json<Page<Todo>>(200, path, admin)).items.map(({ id }) => id)).toEqual([
      first!.id,
    ]);
  });
});
