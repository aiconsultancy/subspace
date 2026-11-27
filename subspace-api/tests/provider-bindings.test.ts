import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { Effect } from "effect";
import { Database, databaseLayer } from "../src/db";
import { bootstrapOwner } from "../src/pairing-owner";
import {
  providerOrganisationBindings,
  releaseProviderOrganisation,
  ReleaseRefused,
} from "../src/provider-organisations";
import { agentKey, deviceToken, type AgentKey } from "./certificates";
import { entraTenant, isolatedDatabase, startApi, testDatabase } from "./entra-stack";

const deviceAudience = "https://subspace.example.internal";

describe.skipIf(!testDatabase)(
  "binding a tenant to a pairing organisation, and releasing it",
  () => {
    let database: Awaited<ReturnType<typeof isolatedDatabase>>;
    let tenant: Awaited<ReturnType<typeof entraTenant>>;
    let api: Awaited<ReturnType<typeof startApi>>;
    let owner: { id: string; key: AgentKey };

    const run = <A, E>(effect: Effect.Effect<A, E, Database>) =>
      Effect.runPromise(effect.pipe(Effect.provide(databaseLayer(database.url, 2))));

    beforeAll(async () => {
      database = await isolatedDatabase("provider_bindings");
      tenant = await entraTenant();
      api = await startApi(
        database.url,
        { ...tenant.env, AUTH_PROVIDERS: "entra,pairing", DEVICE_TOKEN_AUDIENCE: deviceAudience },
        tenant.fetchRemote,
      );
      const { invite, owner: created } = await run(
        bootstrapOwner({
          externalOrganisation: "example",
          displayName: "Tamsin Pryor",
          lifetime: 3600,
          reserved: new Set(),
        }),
      );
      const key = agentKey();
      const response = await api.call("/api/v1/pairing/redeem", undefined, "POST", {
        invite: invite.token,
        certificate: key.certificate,
      });
      expect(response.status).toBe(201);
      owner = { id: created.id, key };
    });
    afterAll(async () => {
      await api?.close();
      await database?.drop();
    });

    const call = async (token: string, path: string, method = "GET", body?: unknown) => {
      const response = await api.call(path, token, method, body);
      return { status: response.status, body: await response.json() };
    };
    /** A person the owner invites, with a paired device. */
    const invitedPerson = async (displayName: string) => {
      const ownerToken = await deviceToken(owner.key, deviceAudience);
      const issued = await call(ownerToken, "/api/v1/pairing/invites", "POST", { displayName });
      expect(issued.status).toBe(201);
      const key = agentKey();
      const response = await api.call("/api/v1/pairing/redeem", undefined, "POST", {
        invite: issued.body.invite,
        certificate: key.certificate,
      });
      expect(response.status).toBe(201);
      return { id: issued.body.memberId as string, key };
    };
    const link = async (oid: string, claims: Record<string, unknown> = {}) =>
      call(await deviceToken(owner.key, deviceAudience), "/api/v1/me/identities", "POST", {
        token: await tenant.sign(oid, claims),
      });

    test("an administrator binds the tenant only with a member's account; the operator releases it", async () => {
      // Only a token whose acct claim says its account is the tenant's member binds the tenant:
      // not a guest's, a foreign home tenant's, nor one without acct, whatever its idp says.
      const ownIssuer = `https://login.microsoftonline.com/${tenant.tenant}/v2.0`;
      for (const claims of [
        {},
        { idp: ownIssuer },
        { acct: 1 },
        { acct: 1, idp: ownIssuer },
        { idp: "https://sts.windows.net/another-tenant/" },
      ]) {
        const refused = await link(crypto.randomUUID(), claims);
        expect(refused.status).toBe(403);
        expect(refused.body.detail).toContain("acct");
      }
      expect(await run(providerOrganisationBindings)).toEqual([]);

      const oid = crypto.randomUUID();
      const linked = await link(oid, { acct: 0 });
      expect(linked.status).toBe(201);
      // Bound: the account signs in as the owner, and an unknown person of the tenant is refused.
      expect((await call(await tenant.sign(oid), "/api/v1/me")).body.userId).toBe(owner.id);
      expect((await call(await tenant.sign(crypto.randomUUID()), "/api/v1/me")).status).toBe(403);
      // Once bound, a member links their own account without acct, as before.
      const person = await invitedPerson("Kerensa Daw");
      const theirs = await call(
        await deviceToken(person.key, deviceAudience),
        "/api/v1/me/identities",
        "POST",
        { token: await tenant.sign(crypto.randomUUID()) },
      );
      expect(theirs.status).toBe(201);
      expect(await run(providerOrganisationBindings)).toEqual([
        {
          organisation: { provider: "pairing", externalId: "example" },
          provider: "entra",
          externalOrganisation: tenant.tenant,
          accounts: 2,
          own: false,
        },
      ]);

      await expect(
        run(
          releaseProviderOrganisation(
            { provider: "pairing", externalId: "missing" },
            { provider: "entra", externalOrganisation: tenant.tenant },
          ),
        ),
      ).rejects.toBeInstanceOf(ReleaseRefused);
      const released = await run(
        releaseProviderOrganisation(
          { provider: "pairing", externalId: "example" },
          { provider: "entra", externalOrganisation: tenant.tenant },
        ),
      );
      expect(released.toSorted()).toEqual([owner.id, person.id].toSorted());
      const audits = (
        await database.client.query(
          "SELECT \"actorType\" AS actor, actor_display_name, source, entity_id, changes FROM audit_logs WHERE action = 'memberIdentityUnlinked' AND entity_id = $1",
          [linked.body.id],
        )
      ).rows;
      expect(audits).toEqual([
        {
          actor: "integration",
          actor_display_name: "Provider bindings command",
          source: "cli",
          entity_id: linked.body.id,
          changes: {
            memberId: owner.id,
            provider: "entra",
            externalOrganisation: tenant.tenant,
            subject: oid,
          },
        },
      ]);
      expect(await run(providerOrganisationBindings)).toEqual([]);
      // Released: the account no longer signs in as the owner, whose device still works, and the
      // tenant's people are registered in its own organisation again.
      const after = await call(await tenant.sign(oid), "/api/v1/me");
      expect(after.status).toBe(200);
      expect(after.body.userId).not.toBe(owner.id);
      expect(
        (await call(await deviceToken(owner.key, deviceAudience), "/api/v1/me")).body.userId,
      ).toBe(owner.id);
      // Releasing again unlinks nothing.
      expect(
        await run(
          releaseProviderOrganisation(
            { provider: "pairing", externalId: "example" },
            { provider: "entra", externalOrganisation: tenant.tenant },
          ),
        ),
      ).toEqual([]);
    });
  },
);
