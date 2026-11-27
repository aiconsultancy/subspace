import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { Effect } from "effect";
import { type RemoteFetch } from "../src/auth";
import { databaseLayer } from "../src/db";
import { bootstrapOwner } from "../src/pairing-owner";
import { agentKey, deviceToken, type AgentKey } from "./certificates";
import {
  acrossOrganisationLock,
  entraTenant,
  isolatedDatabase,
  startApi,
  testDatabase,
  workosEnvironment,
} from "./entra-stack";

/**
 * Linking and unlinking through the API, with another write committed while a request waits for
 * its organisation's lock: a link reads its provider organisation's claim under that provider
 * organisation's lock, which an unlink takes too, and checks its signing device again under the
 * organisation's lock.
 */

const deviceAudience = "https://subspace.example.internal";

describe.skipIf(!testDatabase)("linking against concurrent writes", () => {
  let database: Awaited<ReturnType<typeof isolatedDatabase>>;
  let workos: Awaited<ReturnType<typeof workosEnvironment>>;
  let api: Awaited<ReturnType<typeof startApi>>;
  let organisationId: string;
  let owner: { id: string; key: AgentKey };

  beforeAll(async () => {
    database = await isolatedDatabase("link_serialisation");
    const tenant = await entraTenant();
    workos = await workosEnvironment();
    const fetchRemote: RemoteFetch = async (url, init) =>
      url === workos.jwksUrl
        ? Response.json({ keys: [workos.jwk] })
        : tenant.fetchRemote(url, init);
    api = await startApi(
      database.url,
      {
        ...tenant.env,
        NODE_ENV: "production",
        AUTH_PROVIDERS: "entra,workos,pairing",
        WORKOS_CLIENT_ID: "client_example",
        WORKOS_JWKS_URL: workos.jwksUrl,
        DEVICE_TOKEN_AUDIENCE: deviceAudience,
      },
      fetchRemote,
    );
    const bootstrapped = await Effect.runPromise(
      bootstrapOwner({
        externalOrganisation: "serial",
        displayName: "Demelza Rowe",
        email: null,
        lifetime: 3600,
        reserved: new Set(),
      }).pipe(Effect.provide(databaseLayer(database.url, 2))),
    );
    organisationId = (
      await database.client.query("SELECT organisation_id FROM users WHERE id = $1", [
        bootstrapped.owner.id,
      ])
    ).rows[0].organisation_id;
    const key = agentKey();
    await redeem(bootstrapped.invite.token, key);
    owner = { id: bootstrapped.owner.id, key };
  });
  afterAll(async () => {
    await api?.close();
    await database?.drop();
  });

  const redeem = async (invite: string, key: AgentKey) => {
    const response = await api.call("/api/v1/pairing/redeem", undefined, "POST", {
      invite,
      certificate: key.certificate,
    });
    expect(response.status).toBe(201);
    return (await response.json()).credentialId as string;
  };
  const call = async (key: AgentKey, path: string, method = "GET", body?: unknown) => {
    const response = await api.call(path, await deviceToken(key, deviceAudience), method, body);
    return { status: response.status, body: await response.json() };
  };
  /** A person the owner invites, with one paired device, made an administrator if asked. */
  const person = async (displayName: string, role: "admin" | "member" = "member") => {
    const issued = await call(owner.key, "/api/v1/pairing/invites", "POST", { displayName });
    expect(issued.status).toBe(201);
    await database.client.query("UPDATE users SET role = $2 WHERE id = $1", [
      issued.body.memberId,
      role,
    ]);
    const key = agentKey();
    return {
      id: issued.body.memberId as string,
      key,
      credentialId: await redeem(issued.body.invite, key),
    };
  };
  const link = async (key: AgentKey, subject: string, workosOrganisation: string) =>
    call(key, "/api/v1/me/identities", "POST", {
      token: await workos.sign(subject, workosOrganisation),
    });
  const linksOf = async (workosOrganisation: string) =>
    (
      await database.client.query(
        "SELECT id, user_id, subject FROM member_identities WHERE provider = 'workos' AND external_organisation = $1",
        [workosOrganisation],
      )
    ).rows;
  /** `first`, then `second` once `first` waits, both held at the organisation's lock until then. */
  const both = async <A, B>(first: () => Promise<A>, second: () => Promise<B>) => {
    let started: Promise<B> | undefined;
    const done = await acrossOrganisationLock(
      database.url,
      organisationId,
      first,
      async (waiters) => {
        started = second();
        started.catch(() => {});
        await waiters(2);
      },
    );
    return [done, await started!] as const;
  };

  test("a link refuses a device revoked after the request signed in", async () => {
    const admin = await person("Jory Kemp", "admin");
    const subject = `user_${crypto.randomUUID()}`;
    const refused = await acrossOrganisationLock(
      database.url,
      organisationId,
      () => link(admin.key, subject, "org_revoked"),
      () =>
        database.client.query("UPDATE agent_credentials SET revoked_at = now() WHERE id = $1", [
          admin.credentialId,
        ]),
    );
    expect(refused.status).toBe(403);
    expect(await linksOf("org_revoked")).toEqual([]);
    const audits = await database.client.query(
      "SELECT id FROM audit_logs WHERE action = 'memberIdentityLinked' AND changes->>'subject' = $1",
      [subject],
    );
    expect(audits.rows).toEqual([]);
  });

  test("linking an account again while its last link is unlinked never answers with the deleted link", async () => {
    const admin = await person("Loveday Opie", "admin");
    const subject = `user_${crypto.randomUUID()}`;
    const first = await link(admin.key, subject, "org_repeat");
    expect(first.status).toBe(201);
    const [unlinked, again] = await both(
      () => call(admin.key, `/api/v1/me/identities/${first.body.id}`, "DELETE"),
      () => link(admin.key, subject, "org_repeat"),
    );
    expect(unlinked.status).toBe(200);
    // The unlink came first: the link is the provider organisation's first again, which the
    // administrator may make, and it answers with the link it made, not the one deleted.
    expect(again.status).toBe(201);
    expect(again.body.id).not.toBe(first.body.id);
    expect(await linksOf("org_repeat")).toEqual([
      { id: again.body.id, user_id: admin.id, subject },
    ]);
  });

  test("a member's link after the last link is unlinked needs an administrator again", async () => {
    const admin = await person("Talan Nance", "admin");
    const member = await person("Elowen Bray");
    const first = await link(admin.key, `user_${crypto.randomUUID()}`, "org_released");
    expect(first.status).toBe(201);
    const [unlinked, refused] = await both(
      () => call(admin.key, `/api/v1/me/identities/${first.body.id}`, "DELETE"),
      () => link(member.key, `user_${crypto.randomUUID()}`, "org_released"),
    );
    expect(unlinked.status).toBe(200);
    // The binding was released before the member's link read it: theirs would be the first.
    expect(refused.status).toBe(403);
    expect(await linksOf("org_released")).toEqual([]);
  });
});
