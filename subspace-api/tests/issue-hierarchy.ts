import { expect, test } from "bun:test";
import pg from "pg";
import { localDatabase } from "./local-database";

type Json = <T>(
  status: number,
  path: string,
  token: string,
  method?: string,
  data?: unknown,
) => Promise<T>;
type Fixture = {
  admin: string;
  adminId: string;
  key: string;
  ordinaryId: string;
  projectId: string;
  runId: string;
};
type Issue = {
  id: string;
  identifier: string;
  version: number;
  state: { id: string; type: string };
};
type Page<T> = { items: T[] };

export function testIssueHierarchy(json: Json, fixture: () => Fixture) {
  test("parent candidates share the write-time ancestry rule", async () => {
    const { admin, projectId, runId } = fixture();
    const tag = `Parent search ${runId}`;
    const tracker = (name: string, parentId?: string) =>
      json<Issue>(201, "/issues", admin, "POST", {
        projectId,
        type: "tracker",
        title: `${tag} ${name}`,
        summary: "Parent filter fixture",
        parentId,
      });
    const moving = await tracker("root");
    const middle = await tracker("child", moving.id);
    const leaf = await tracker("grandchild", middle.id);
    const archived = await tracker("archived root");
    const hidden = await tracker("under archived root", archived.id);
    const eligible = await tracker("eligible");
    await json(200, `/issues/${archived.id}/archive`, admin, "POST", { version: archived.version });
    const list = `/issues?type=tracker&q=${encodeURIComponent(tag)}&includeArchived=true`;
    const ids = async (query: string) =>
      (await json<Page<Issue>>(200, `${list}&${query}`, admin)).items.map(({ id }) => id);
    expect(await ids(`parentFor=${moving.identifier}`)).toEqual([eligible.id]);
    expect(new Set(await ids("parentFor=new"))).toEqual(
      new Set([moving.id, middle.id, leaf.id, eligible.id]),
    );
    expect(await ids("unsupportedFilter=ignored")).toHaveLength(6);
    const detail = "The parent is archived or would create a cycle.";
    for (const candidate of [moving, leaf, hidden]) {
      const refused = await json(409, `/issues/${moving.id}`, admin, "PATCH", {
        version: moving.version,
        parentId: candidate.id,
      });
      expect(refused).toMatchObject({ detail, errors: { parentId: [detail] } });
    }
    const refusedCreate = await json(409, "/issues", admin, "POST", {
      projectId,
      title: `${tag} refused create`,
      summary: "Archived parent fixture",
      parentId: archived.id,
    });
    expect(refusedCreate).toMatchObject({ detail, errors: { parentId: [detail] } });
    await json(200, `/issues/${moving.id}`, admin, "PATCH", {
      version: moving.version,
      parentId: eligible.id,
    });
  });

  const database = process.env.TEST_DATABASE_URL;
  test.skipIf(!database)("parent candidates exclude pre-existing cycles", async () => {
    const local = localDatabase(database!);
    const { admin, projectId, runId } = fixture();
    const tag = `Cyclic candidates ${runId}`;
    const tracker = (name: string, parentId?: string) =>
      json<Issue>(201, "/issues", admin, "POST", {
        projectId,
        type: "tracker",
        title: `${tag} ${name}`,
        summary: "Cycle fixture",
        parentId,
      });
    const root = await tracker("root");
    const child = await tracker("child", root.id);
    const eligible = await tracker("eligible");
    const db = new pg.Client(local.settings());
    await db.connect();
    try {
      // Normal API writes cannot produce a cycle. Seed one in these two scratch rows only.
      const seeded = await db.query(
        "update issues set parent_id = $2 where id = $1 and parent_id is null",
        [root.id, child.id],
      );
      expect(seeded.rowCount).toBe(1);
      const page = await json<{ items: Issue[] }>(
        200,
        `/issues?type=tracker&parentFor=new&q=${encodeURIComponent(tag)}`,
        admin,
      );
      expect(page.items.map(({ id }) => id)).toEqual([eligible.id]);
      await json(409, `/issues/${eligible.id}`, admin, "PATCH", {
        version: eligible.version,
        parentId: child.id,
      });
    } finally {
      try {
        await db.query("update issues set parent_id = null where id = $1 and parent_id = $2", [
          root.id,
          child.id,
        ]);
      } finally {
        await db.end();
      }
    }
  });

  test("tracker states follow children and issue archive retains attribution", async () => {
    const { admin, adminId, key, projectId } = fixture();
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

  test("tracker assignment and derived state update children only with confirmation", async () => {
    const { admin, adminId, key, ordinaryId, projectId } = fixture();
    type Assigned = Issue & { assignee: { userId: string } | null; readyForAgents: boolean };
    const states = await json<Page<{ id: string; type: string }>>(
      200,
      `/projects/${key}/workflow-states`,
      admin,
    );
    const backlog = states.items.find((state) => state.type === "backlog")!.id;
    const todo = states.items.find((state) => state.type === "unstarted")!.id;
    const done = states.items.find((state) => state.type === "completed")!.id;
    const parent = (title: string) =>
      json<Issue>(201, "/issues", admin, "POST", {
        projectId,
        type: "tracker",
        title,
        summary: title,
      });
    const previousParent = await parent("Previous parent fixture");
    const nextParent = await parent("Next parent fixture");
    const tracker = await json<Assigned>(201, "/issues", admin, "POST", {
      projectId,
      type: "tracker",
      title: "Coordinator fixture",
      summary: "Group child changes",
      parentId: previousParent.id,
    });
    const child = (title: string, stateId: string, fields = {}) =>
      json<Assigned>(201, "/issues", admin, "POST", {
        projectId,
        title,
        summary: title,
        parentId: tracker.id,
        stateId,
        ...fields,
      });
    const waiting = await child("Waiting fixture", backlog, { readyForAgents: true });
    const assigned = await child("Already assigned", backlog, { assigneeId: adminId });
    const completed = await child("Already completed", done);
    const archived = await child("Archived child", backlog);
    await json(200, `/issues/${archived.id}/archive`, admin, "POST", { version: archived.version });

    const read = (id: string) => json<Assigned>(200, `/issues/${id}`, admin);
    const patch = (status: number, version: number, changes: object) =>
      json<Assigned>(status, `/issues/${tracker.id}`, admin, "PATCH", { version, ...changes });
    const current = await read(tracker.id);
    const previousParentBefore = await read(previousParent.id);
    const onlyTracker = await patch(200, current.version, { assigneeId: ordinaryId });
    expect(onlyTracker.assignee?.userId).toBe(ordinaryId);
    expect((await read(waiting.id)).assignee).toBeNull();

    await patch(400, onlyTracker.version, { moveOpenChildren: false });
    await patch(400, onlyTracker.version, { assignUnassignedChildren: false });
    expect((await read(tracker.id)).version).toBe(onlyTracker.version);
    await patch(400, onlyTracker.version, { stateId: todo });
    const together = {
      parentId: nextParent.identifier,
      assigneeId: ordinaryId,
      assignUnassignedChildren: true,
      stateId: todo,
      moveOpenChildren: true,
    };
    await patch(409, current.version, together);
    await patch(409, onlyTracker.version, { ...together, parentId: waiting.id });
    expect(await read(tracker.id)).toMatchObject({
      parentId: previousParent.id,
      version: onlyTracker.version,
    });
    expect(await read(waiting.id)).toMatchObject({
      assignee: null,
      state: { id: backlog },
      version: waiting.version,
    });
    expect(await read(nextParent.id)).toMatchObject({
      state: { id: backlog },
      version: nextParent.version,
    });

    const updated = await patch(200, onlyTracker.version, together);
    expect(updated).toMatchObject({ parentId: nextParent.id, version: onlyTracker.version + 1 });
    expect(updated.assignee?.userId).toBe(ordinaryId);
    expect(updated.state.type).toBe("started");
    expect(await read(previousParent.id)).toMatchObject({
      state: { id: backlog },
      version: previousParentBefore.version + 1,
    });
    expect(await read(nextParent.id)).toMatchObject({
      state: { type: "started" },
      version: nextParent.version + 1,
    });
    expect(await read(waiting.id)).toMatchObject({
      assignee: { userId: ordinaryId },
      state: { id: todo },
      readyForAgents: false,
      version: waiting.version + 1,
    });
    expect(await read(assigned.id)).toMatchObject({
      assignee: { userId: adminId },
      state: { id: todo },
    });
    expect((await read(completed.id)).state.id).toBe(done);
    expect(await read(archived.id)).toMatchObject({ assignee: null, state: { id: backlog } });
    const activity = await json<Page<{ kind: string }>>(
      200,
      `/issues/${waiting.id}/activity`,
      admin,
    );
    expect(activity.items.some((item) => item.kind === "issue.changed")).toBe(true);

    const childProject = await json<{ id: string; version: number }>(
      201,
      "/projects",
      admin,
      "POST",
      {
        key: "SIDE",
        name: "Child project fixture",
      },
    );
    await json<Assigned>(201, "/issues", admin, "POST", {
      projectId: childProject.id,
      title: "A child in another project",
      summary: "Check atomic refusal.",
      parentId: tracker.id,
    });
    await json(200, `/projects/${childProject.id}/archive`, admin, "POST", {
      version: childProject.version,
    });
    const unchanged = await read(tracker.id);
    const refused = await json<unknown>(400, `/issues/${tracker.id}`, admin, "PATCH", {
      version: unchanged.version,
      parentId: null,
      stateId: backlog,
      moveOpenChildren: true,
    });
    expect(refused).toMatchObject({
      errors: { trackerChildren: ["A child belongs to an archived project."] },
    });
    expect(await read(tracker.id)).toMatchObject({
      parentId: nextParent.id,
      version: unchanged.version,
    });
    expect((await read(waiting.id)).state.id).toBe(todo);
  });
}
