import { expect, test } from "bun:test";
import { mkdtemp, readdir, rm, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { applyMigrations, isolatedDatabase, testDatabase } from "./entra-stack";

const folder = fileURLToPath(new URL("../drizzle", import.meta.url));

test.skipIf(!testDatabase)(
  "the linked account and invite issuer migrations widen paired devices' scopes only, and apply again",
  async () => {
    const database = await isolatedDatabase("linked_accounts_migration", false);
    try {
      const directories = (await readdir(folder)).sort();
      const first = directories.find((name) => name.endsWith("_member_identities"))!;
      expect(first).toBeDefined();
      expect(directories.some((name) => name.endsWith("_invite_issuing_device"))).toBe(true);
      const previous = await mkdtemp(join(tmpdir(), "subspace-drizzle-"));
      try {
        for (const name of directories.filter((name) => name < first))
          await symlink(join(folder, name), join(previous, name), "dir");
        await applyMigrations(database.url, previous);
      } finally {
        await rm(previous, { recursive: true });
      }

      // A pairing organisation as the first pull request left it: an owner with a paired device
      // and a revoked one, an agent with a key credential, and a pending invite.
      const { client } = database;
      const one = async (text: string, values: unknown[] = []) =>
        (await client.query(text, values)).rows[0];
      const { id: org } = await one(
        "INSERT INTO organisations (provider, external_id) VALUES ('pairing', 'existing') RETURNING id",
      );
      const ownerId = crypto.randomUUID();
      await client.query(
        "INSERT INTO users (id, organisation_id, subject, display_name, role, username) VALUES ($1, $2, $3, 'Owner', 'admin', 'owner')",
        [ownerId, org, `subspace-member:${ownerId}`],
      );
      const { id: fleet } = await one(
        "INSERT INTO fleets (organisation_id, slug, name) VALUES ($1, 'alpha', 'Alpha') RETURNING id",
        [org],
      );
      const { id: agent } = await one(
        `INSERT INTO users (organisation_id, subject, display_name, agent_rank, agent_fleet, agent_fleet_id, agent_model, registered_by_id, agent_irc_nick)
         VALUES ($1, 'agent@example.com', 'Kestrel', 'crew', 'alpha', $2, 'example-model', $3, 'Kestrel') RETURNING id`,
        [org, fleet, ownerId],
      );
      const invite = (hash: string) =>
        one(
          `INSERT INTO pairing_invites (organisation_id, user_id, token_hash, expires_at)
           VALUES ($1, $2, $3, now() + interval '1 hour') RETURNING id`,
          [org, ownerId, hash],
        );
      const credential = (user: string, fingerprint: string, inviteId: string | null) =>
        one(
          `INSERT INTO agent_credentials (organisation_id, user_id, fingerprint, algorithm, certificate, public_key, scopes, pairing_invite_id)
           VALUES ($1, $2, $3, 'EdDSA', 'certificate', 'key', '{api}', $4) RETURNING id`,
          [org, user, fingerprint, inviteId],
        );
      const desk = await credential(ownerId, "a".repeat(64), (await invite("1".repeat(64))).id);
      const lost = await credential(ownerId, "b".repeat(64), (await invite("2".repeat(64))).id);
      await client.query("UPDATE agent_credentials SET revoked_at = now() WHERE id = $1", [
        lost.id,
      ]);
      const key = await credential(agent, "c".repeat(64), null);
      const pending = await invite("3".repeat(64));
      const credentials = async () =>
        Object.fromEntries(
          (await client.query("SELECT id, scopes, revoked_at FROM agent_credentials")).rows.map(
            (row) => [row.id, { scopes: row.scopes, revoked: row.revoked_at !== null }],
          ),
        );
      const before = await credentials();

      await applyMigrations(database.url, folder);
      await applyMigrations(database.url, folder);

      // Paired devices, revoked or not, may now ask for IRC passwords; an agent's credential and
      // every revocation are unchanged.
      expect(await credentials()).toEqual({
        [desk.id]: { scopes: ["api", "irc"], revoked: false },
        [lost.id]: { scopes: ["api", "irc"], revoked: true },
        [key.id]: before[key.id],
      });
      // Existing invites name no issuing device; a new one may name a device of its organisation
      // only.
      expect(
        await one("SELECT created_by_credential_id FROM pairing_invites WHERE id = $1", [
          pending.id,
        ]),
      ).toEqual({ created_by_credential_id: null });
      await client.query("UPDATE pairing_invites SET created_by_credential_id = $1 WHERE id = $2", [
        desk.id,
        pending.id,
      ]);
      const { id: other } = await one(
        "INSERT INTO organisations (provider, external_id) VALUES ('pairing', 'other') RETURNING id",
      );
      const strangerId = crypto.randomUUID();
      await client.query(
        "INSERT INTO users (id, organisation_id, subject, display_name, username) VALUES ($1, $2, $3, 'Stranger', 'stranger')",
        [strangerId, other, `subspace-member:${strangerId}`],
      );
      await expect(
        client.query(
          `INSERT INTO pairing_invites (organisation_id, user_id, token_hash, expires_at, created_by_credential_id)
           VALUES ($1, $2, $3, now() + interval '1 hour', $4)`,
          [other, strangerId, "4".repeat(64), desk.id],
        ),
      ).rejects.toThrow();
    } finally {
      await database.drop();
    }
  },
);
