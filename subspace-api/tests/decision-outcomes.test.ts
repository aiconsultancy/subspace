import { expect, test } from "bun:test";
import { entraTenant, isolatedDatabase, startApi, testDatabase } from "./entra-stack";

test.skipIf(!testDatabase)(
  "only the captain's fleet can deliver pending decision outcomes",
  async () => {
    const database = await isolatedDatabase("decision_outcomes");
    let api: Awaited<ReturnType<typeof startApi>> | undefined;
    try {
      const { client } = database;
      const tenant = await entraTenant();
      const [{ id: org }] = (
        await client.query(
          "INSERT INTO organisations (provider, external_id) VALUES ('entra', $1) RETURNING id",
          [tenant.tenant],
        )
      ).rows;
      const [{ id: ownFleet }, { id: otherFleet }] = (
        await client.query(
          "INSERT INTO fleets (organisation_id, slug, name) VALUES ($1, 'alpha', 'Alpha'), ($1, 'beta', 'Beta') RETURNING id",
          [org],
        )
      ).rows;
      const ownerSubject = crypto.randomUUID();
      const [{ id: owner }] = (
        await client.query(
          "INSERT INTO users (organisation_id, subject, display_name, role) VALUES ($1, $2, 'Example Owner', 'admin') RETURNING id",
          [org, ownerSubject],
        )
      ).rows;
      const [{ id: project }] = (
        await client.query(
          `INSERT INTO projects (organisation_id, key, name, lead_id, created_by_user_id, default_state_id)
         VALUES ($1, 'NAV', 'Navigation', $2, $2, gen_random_uuid()) RETURNING id`,
          [org, owner],
        )
      ).rows;
      async function officer(name: string, rank: string, fleet: string, slug: string) {
        const subject = crypto.randomUUID();
        const [{ id }] = (
          await client.query(
            `INSERT INTO users (organisation_id, subject, display_name, agent_rank, agent_fleet_id,
             agent_fleet, agent_model, registered_by_id, agent_irc_nick)
           VALUES ($1, $2, $3, $4, $5, $6, 'example-model', $7, $3) RETURNING id`,
            [org, subject, name, rank, fleet, slug, owner],
          )
        ).rows;
        return { id, token: await tenant.sign(subject) };
      }
      const captain = await officer("Pioneer", "captain", ownFleet, "alpha");
      const dispatcher = await officer("Polaris", "rearAdmiral", ownFleet, "alpha");
      const recorder = await officer("Beacon", "rearAdmiral", ownFleet, "alpha");
      const outsider = await officer("Horizon", "rearAdmiral", otherFleet, "beta");
      const [first, second, , deferred] = (
        await client.query(
          `INSERT INTO project_decisions
           (organisation_id, project_id, recorded_by_id, identifier, title, decision, decided_on, made_by_type, status)
         VALUES ($1, $2, $3, 'D-00001-NAV', 'First', 'Set the route', '2026-01-01', 'human', 'proposed'),
                ($1, $2, $3, 'D-00002-NAV', 'Second', 'Set a new route', '2026-01-01', 'human', 'proposed'),
                ($1, $2, $3, 'D-00003-NAV', 'Third', 'Keep the existing route', '2026-01-01', 'human', 'accepted'),
                ($1, $2, $4, 'D-00004-NAV', 'Fourth', 'Record officer advice', '2026-01-01', 'human', 'proposed')
         RETURNING id, identifier`,
          [org, project, captain.id, recorder.id],
        )
      ).rows;

      api = await startApi(database.url, tenant.env, tenant.fetchRemote);
      const ownerToken = await tenant.sign(ownerSubject);
      const path = `/api/v1/fleets/${ownFleet}/decision-outcomes`;
      const page = async (suffix = "") => {
        const response = await api!.call(path + suffix, dispatcher.token);
        expect(response.status).toBe(200);
        return response.json();
      };
      expect(await page()).toEqual({ items: [], nextCursor: null });

      for (const [decision, status] of [
        [first, "accepted"],
        [second, "rejected"],
        [deferred, "accepted"],
      ]) {
        const response = await api.call(
          `/api/v1/decisions/${decision.identifier}/status`,
          ownerToken,
          "PUT",
          { status, version: 1 },
        );
        expect(response.status).toBe(200);
      }
      await client.query("UPDATE users SET agent_rank = 'captain' WHERE id = $1", [recorder.id]);
      expect(
        (await api.call(`${path}/${deferred.id}/delivered`, dispatcher.token, "POST")).status,
      ).toBe(404);
      const firstPage = await page("?limit=1");
      expect(firstPage.items).toMatchObject([
        { ...second, recordedByUserId: captain.id, status: "rejected" },
      ]);
      expect(firstPage.nextCursor).toBeString();
      expect(
        (await page(`?limit=1&cursor=${encodeURIComponent(firstPage.nextCursor)}`)).items,
      ).toMatchObject([{ ...first, recordedByUserId: captain.id, status: "accepted" }]);

      const receipt = `${path}/${second.id}/delivered`;
      for (const token of [ownerToken, captain.token, outsider.token]) {
        expect((await api.call(path, token)).status).toBe(403);
        expect((await api.call(receipt, token, "POST")).status).toBe(403);
      }
      expect((await api.call(receipt, dispatcher.token, "POST")).status).toBe(204);
      expect((await api.call(receipt, dispatcher.token, "POST")).status).toBe(204);
      expect((await page()).items.map((item: { id: string }) => item.id)).toEqual([first.id]);
      await client.query("UPDATE users SET agent_rank = 'rearAdmiral' WHERE id = $1", [captain.id]);
      expect((await api.call(receipt, dispatcher.token, "POST")).status).toBe(204);
    } finally {
      await api?.close();
      await database.drop();
    }
  },
);
