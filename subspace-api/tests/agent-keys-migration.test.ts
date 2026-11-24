import { expect, test } from "bun:test";
import { mkdtemp, readdir, rm, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { applyMigrations, isolatedDatabase, testDatabase } from "./entra-stack";

const folder = fileURLToPath(new URL("../drizzle", import.meta.url));

test.skipIf(!testDatabase)(
  "the agent key and enrolment migrations apply to a database with data, and again",
  async () => {
    const database = await isolatedDatabase("agent_keys_migration", false);
    try {
      const directories = (await readdir(folder)).sort();
      const first = directories.find((name) => name.endsWith("_agent_credentials"))!;
      expect(first).toBeDefined();
      expect(directories.some((name) => name.endsWith("_enrolment_tokens"))).toBe(true);
      const previous = await mkdtemp(join(tmpdir(), "subspace-drizzle-"));
      try {
        for (const name of directories.filter((name) => name < first))
          await symlink(join(folder, name), join(previous, name), "dir");
        await applyMigrations(database.url, previous);
      } finally {
        await rm(previous, { recursive: true });
      }

      const { client } = database;
      const [{ id: org }] = (
        await client.query(
          "INSERT INTO organisations (provider, external_id) VALUES ('dev', 'existing') RETURNING id",
        )
      ).rows;
      const [{ id: owner }] = (
        await client.query(
          "INSERT INTO users (organisation_id, subject, display_name, role, username) VALUES ($1, 'owner@example.com', 'Owner', 'admin', 'owner') RETURNING id",
          [org],
        )
      ).rows;
      const [{ id: fleet }] = (
        await client.query(
          "INSERT INTO fleets (organisation_id, slug, name) VALUES ($1, 'alpha', 'Alpha') RETURNING id",
          [org],
        )
      ).rows;
      const [{ id: agent }] = (
        await client.query(
          `INSERT INTO users (organisation_id, subject, display_name, agent_rank, agent_fleet, agent_fleet_id, agent_model, registered_by_id, agent_irc_nick)
           VALUES ($1, 'agent@example.com', 'Kestrel', 'rearAdmiral', 'alpha', $2, 'example-model', $3, 'Kestrel') RETURNING id`,
          [org, fleet, owner],
        )
      ).rows;
      const before = (await client.query("SELECT * FROM users ORDER BY id")).rows;

      await applyMigrations(database.url, folder);
      await applyMigrations(database.url, folder);

      expect((await client.query("SELECT * FROM users ORDER BY id")).rows).toEqual(before);
      const [{ id: token }] = (
        await client.query(
          `INSERT INTO enrolment_tokens (organisation_id, fleet_id, token_hash, expires_at, max_uses, created_by_id)
           VALUES ($1, $2, $3, now() + interval '1 hour', 16, $4) RETURNING id`,
          [org, fleet, "a".repeat(64), owner],
        )
      ).rows;
      await client.query(
        `INSERT INTO agent_credentials (organisation_id, user_id, fingerprint, algorithm, certificate, public_key, scopes, enrolment_token_id)
         VALUES ($1, $2, $3, 'EdDSA', 'certificate', 'key', '{api,irc}', $4)`,
        [org, agent, "b".repeat(64), token],
      );
      // The database keeps one active credential per fingerprint, and only the shapes allowed.
      const insert = (fingerprint: string, scopes: string) =>
        client.query(
          `INSERT INTO agent_credentials (organisation_id, user_id, fingerprint, algorithm, certificate, public_key, scopes)
           VALUES ($1, $2, $3, 'EdDSA', 'certificate', 'key', $4)`,
          [org, agent, fingerprint, scopes],
        );
      await expect(insert("b".repeat(64), "{api}")).rejects.toThrow();
      await expect(insert("B".repeat(64), "{api}")).rejects.toThrow();
      await expect(insert("c".repeat(64), "{}")).rejects.toThrow();
      await expect(insert("c".repeat(64), "{admin}")).rejects.toThrow();
      await client.query("UPDATE agent_credentials SET revoked_at = now() WHERE fingerprint = $1", [
        "b".repeat(64),
      ]);
      await insert("b".repeat(64), "{api}");
      await expect(
        client.query("UPDATE enrolment_tokens SET uses = 17 WHERE id = $1", [token]),
      ).rejects.toThrow();
    } finally {
      await database.drop();
    }
  },
);
