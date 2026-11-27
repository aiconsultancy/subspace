import { expect, test } from "bun:test";
import { mkdtemp, readdir, rm, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  applyMigrations,
  entraTenant,
  isolatedDatabase,
  startApi,
  testDatabase,
} from "./entra-stack";

const folder = fileURLToPath(new URL("../drizzle", import.meta.url));

test.skipIf(!testDatabase)(
  "decision references backfill, resolve across REST and MCP, and persist across changes",
  async () => {
    const database = await isolatedDatabase("decision_identifiers", false);
    let api: Awaited<ReturnType<typeof startApi>> | undefined;
    try {
      const previous = await mkdtemp(join(tmpdir(), "subspace-drizzle-"));
      try {
        const migrations = (await readdir(folder)).sort();
        const current = migrations.find((name) => name.endsWith("_decision_identifiers"))!;
        for (const name of migrations.filter((name) => name < current)) {
          await symlink(join(folder, name), join(previous, name), "dir");
        }
        await applyMigrations(database.url, previous);
      } finally {
        await rm(previous, { recursive: true });
      }

      const tenant = await entraTenant();
      const { client } = database;
      const [{ id: org }] = (
        await client.query(
          "INSERT INTO organisations (provider, external_id) VALUES ('entra', $1) RETURNING id",
          [tenant.tenant],
        )
      ).rows;
      const [{ id: owner }] = (
        await client.query(
          "INSERT INTO users (organisation_id, subject, display_name, role) VALUES ($1, 'existing', 'Example Owner', 'admin') RETURNING id",
          [org],
        )
      ).rows;
      const [{ id: project }, { id: anotherProject }] = (
        await client.query(
          `INSERT INTO projects (organisation_id, key, name, lead_id, created_by_user_id, default_state_id)
           VALUES ($1, 'EXA', 'Example project', $2, $2, gen_random_uuid()),
                  ($1, 'ALT', 'Alternate project', $2, $2, gen_random_uuid()) RETURNING id`,
          [org, owner],
        )
      ).rows;
      const [{ id: first }, { id: second }] = (
        await client.query(
          `INSERT INTO project_decisions
            (organisation_id, project_id, recorded_by_id, title, decision, decided_on, created_at, made_by_type, status)
           VALUES ($1, $2, $3, 'First', 'Do the first thing', '2026-01-03', '2026-01-01', 'human', 'accepted'),
                  ($1, $2, $3, 'Second', 'Do the second thing', '2026-01-01', '2026-01-02', 'human', 'proposed')
           RETURNING id`,
          [org, project, owner],
        )
      ).rows;
      const original = {
        title: "Second",
        supersedes: { id: first, title: "First", status: "accepted" },
      };
      await client.query(
        `INSERT INTO project_decision_revisions
          (organisation_id, decision_id, number, snapshot, saved_by, changed_fields)
         VALUES ($1, $2, 1, $3, '{}', '["created"]')`,
        [org, second, original],
      );

      await applyMigrations(database.url);
      expect(
        (await client.query("SELECT id, identifier FROM project_decisions ORDER BY identifier"))
          .rows,
      ).toEqual([
        { id: first, identifier: "D-00001-EXA" },
        { id: second, identifier: "D-00002-EXA" },
      ]);
      expect((await client.query("SELECT snapshot FROM project_decision_revisions")).rows).toEqual([
        {
          snapshot: {
            ...original,
            identifier: "D-00002-EXA",
            supersedes: { ...original.supersedes, identifier: "D-00001-EXA" },
          },
        },
      ]);

      api = await startApi(database.url, tenant.env, tenant.fetchRemote);
      const token = await tenant.sign(crypto.randomUUID(), {
        preferred_username: "example@example.com",
        name: "Example User",
      });
      const call = async (status: number, path: string, method = "GET", data?: unknown) => {
        const response = await api!.call(`/api/v1/decisions${path}`, token, method, data);
        const body = await response.json();
        expect([response.status, response.status === status ? "" : body]).toEqual([status, ""]);
        return body;
      };
      const payload = {
        projectId: project,
        title: "Replacement",
        decision: "Change the first choice",
        decidedOn: "2026-01-04",
        supersedesId: first,
      };
      const [a, b] = await Promise.all([
        call(201, "", "POST", payload),
        call(201, "", "POST", payload),
      ]);
      expect([a.identifier, b.identifier].sort()).toEqual(["D-00003-EXA", "D-00004-EXA"]);
      expect(a.supersedes.identifier).toBe("D-00001-EXA");
      const independent = { ...payload, projectId: anotherProject, supersedesId: null };
      expect((await call(201, "", "POST", independent)).identifier).toBe("D-00001-ALT");
      await client.query("UPDATE projects SET next_decision_number = 100000 WHERE id = $1", [
        anotherProject,
      ]);
      const sixDigits = await call(201, "", "POST", independent);
      expect(sixDigits.identifier).toBe("D-100000-ALT");
      expect(await call(200, "/D-100000-ALT")).toMatchObject({ id: sixDigits.id });

      const path = `/${a.identifier.toLowerCase()}`;
      expect(await call(200, path)).toEqual(await call(200, `/${a.id}`));
      expect(
        (await call(200, `?q=${a.identifier}`)).items.map(({ id }: { id: string }) => id),
      ).toEqual([a.id]);
      const updated = await call(200, path, "PUT", { version: a.version, title: "New title" });
      const accepted = await call(200, `${path}/status`, "PUT", {
        version: updated.version,
        status: "accepted",
      });
      expect(accepted).toMatchObject({ identifier: a.identifier, status: "accepted" });
      expect((await call(200, "/D-00001-EXA")).supersededBy.identifier).toBe(a.identifier);

      const history = await call(200, `${path}/revisions?limit=1`);
      expect(
        (await call(200, `/${a.id}/revisions?limit=1&cursor=${history.nextCursor}`)).items,
      ).toHaveLength(1);
      const revision = await call(200, `${path}/revisions/${history.items[0].id}`);
      expect(revision.snapshot).toMatchObject({
        identifier: a.identifier,
        supersedes: { identifier: "D-00001-EXA" },
      });
      const rpc = await api.call(
        "/mcp",
        token,
        "POST",
        {
          jsonrpc: "2.0",
          id: 1,
          method: "tools/call",
          params: { name: "subspace_get_decision", arguments: { decisionId: a.identifier } },
        },
        { Accept: "application/json, text/event-stream" },
      );
      expect(rpc.status).toBe(200);
      expect((await rpc.json()).result.structuredContent).toMatchObject({
        id: a.id,
        identifier: a.identifier,
      });
    } finally {
      await api?.close();
      await database.drop();
    }
  },
);
