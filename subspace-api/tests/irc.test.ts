import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { OpenApi } from "effect/http-api";
import { Redacted } from "effect";
import { documentedApi } from "../src/api";
import { config } from "../src/config";
import {
  ircCredentialLifetimeSeconds,
  mintIrcCredential,
  readIrcCredential,
  sameSecret,
} from "../src/irc";
import { mcpTools } from "../src/mcp-tools";
import { entraTenant, isolatedDatabase, startApi, testDatabase } from "./entra-stack";

const secret = Redacted.make("an example credential secret of at least 32 bytes");
const hookToken = "an-example-hook-token-of-at-least-32-bytes";
const userId = "0f1e2d3c-4b5a-4968-8776-655443322110";
const databaseUrl = "postgresql://subspace@127.0.0.1/subspace";

describe("IRC credentials", () => {
  test("a credential is sirc1. and an opaque payload of at most 200 characters", () => {
    const now = Date.UTC(2026, 10, 23, 21, 0, 0);
    const { password, expiresAt } = mintIrcCredential(secret, userId, now);
    expect(password.startsWith("sirc1.")).toBe(true);
    expect(password.length).toBeLessThanOrEqual(200);
    expect(password).toMatch(/^sirc1\.[A-Za-z0-9_-]+$/u);
    expect(password).not.toContain(userId);
    expect(expiresAt.toISOString()).toBe("2026-11-23T21:10:00.000Z");
    expect(readIrcCredential(secret, password, now)).toEqual({ ok: true, userId });
  });

  test("it is valid for ten minutes and reusable until then", () => {
    const now = Date.now();
    const { password } = mintIrcCredential(secret, userId, now);
    expect(ircCredentialLifetimeSeconds).toBe(600);
    expect(readIrcCredential(secret, password, now).ok).toBe(true);
    expect(readIrcCredential(secret, password, now + 599_000).ok).toBe(true);
    expect(readIrcCredential(secret, password, now + 600_000)).toEqual({
      ok: false,
      reason: "expired",
    });
    // A credential that claims to last longer than a fresh one is refused too.
    const future = mintIrcCredential(secret, userId, now + 3_600_000).password;
    expect(readIrcCredential(secret, future, now)).toEqual({ ok: false, reason: "expired" });
  });

  test("a tampered, foreign or malformed credential is refused", () => {
    const { password } = mintIrcCredential(secret, userId);
    const flip = (index: number) =>
      password.slice(0, index) + (password[index] === "A" ? "B" : "A") + password.slice(index + 1);
    // In the user ID, in the expiry and in the signature.
    for (const index of [8, 30, password.length - 5])
      expect(readIrcCredential(secret, flip(index))).toEqual({ ok: false, reason: "signature" });
    const other = Redacted.make("a different credential secret of 32 bytes or more");
    expect(readIrcCredential(other, password)).toEqual({ ok: false, reason: "signature" });
    for (const malformed of [
      "",
      "sirc1.",
      password.replace("sirc1.", "sirc2."),
      password.slice(0, -1),
      `${password}A`,
      password.replace(/.$/u, "="),
      `${password.slice(0, -1)}+`,
    ])
      expect(readIrcCredential(secret, malformed)).toEqual({ ok: false, reason: "malformed" });
  });

  test("secrets are compared whatever their lengths", () => {
    expect(sameSecret(hookToken, Redacted.make(hookToken))).toBe(true);
    expect(sameSecret(`${hookToken}x`, Redacted.make(hookToken))).toBe(false);
    expect(sameSecret("", Redacted.make(hookToken))).toBe(false);
  });
});

describe("IRC configuration", () => {
  const base = { DATABASE_URL: databaseUrl, AUTH_PROVIDERS: "dev", ENABLE_DEV_AUTH: "true" };
  const hook = {
    ...base,
    IRC_CREDENTIAL_SECRET: Redacted.value(secret),
    IRC_AUTH_HOOK_TOKEN: hookToken,
    IRC_AUTH_ORGANISATION: "dev:local",
  };

  test("nothing is on until configured, and the credential secret works alone", () => {
    expect(config(base).irc).toEqual({ credentialSecret: null, hook: null });
    const alone = config({ ...base, IRC_CREDENTIAL_SECRET: Redacted.value(secret) }).irc;
    expect(Redacted.value(alone.credentialSecret!)).toBe(Redacted.value(secret));
    expect(alone.hook).toBeNull();
    const full = config(hook).irc;
    expect(full.hook).toMatchObject({ provider: "dev", externalOrganisation: "local" });
    expect(Redacted.value(full.hook!.token)).toBe(hookToken);
    // Secrets stay out of logs and JSON.
    expect(JSON.stringify(full)).not.toContain(hookToken);
    expect(JSON.stringify(full)).not.toContain(Redacted.value(secret));
  });

  test("secrets can come from files, and short, empty or doubled secrets are refused", async () => {
    const folder = await mkdtemp(join(tmpdir(), "subspace-irc-"));
    try {
      const secretFile = join(folder, "secret");
      const tokenFile = join(folder, "token");
      await writeFile(secretFile, `${Redacted.value(secret)}\n`);
      await writeFile(tokenFile, `${hookToken}\n`);
      const { IRC_CREDENTIAL_SECRET: _, IRC_AUTH_HOOK_TOKEN: __, ...withoutSecrets } = hook;
      const fromFiles = config({
        ...withoutSecrets,
        IRC_CREDENTIAL_SECRET_FILE: secretFile,
        IRC_AUTH_HOOK_TOKEN_FILE: tokenFile,
      }).irc;
      expect(Redacted.value(fromFiles.credentialSecret!)).toBe(Redacted.value(secret));
      expect(Redacted.value(fromFiles.hook!.token)).toBe(hookToken);
      expect(() => config({ ...hook, IRC_CREDENTIAL_SECRET_FILE: secretFile })).toThrow(
        "Set IRC_CREDENTIAL_SECRET or IRC_CREDENTIAL_SECRET_FILE, not both",
      );
      expect(() =>
        config({ ...base, IRC_CREDENTIAL_SECRET_FILE: join(folder, "missing") }),
      ).toThrow("IRC_CREDENTIAL_SECRET_FILE must name a readable file");
      const empty = join(folder, "empty");
      await writeFile(empty, "\n");
      expect(() => config({ ...base, IRC_CREDENTIAL_SECRET_FILE: empty })).toThrow(
        "must not be empty",
      );
    } finally {
      await rm(folder, { recursive: true });
    }
    expect(() => config({ ...base, IRC_CREDENTIAL_SECRET: "x".repeat(31) })).toThrow(
      "IRC_CREDENTIAL_SECRET must be at least 32 bytes",
    );
    expect(() => config({ ...hook, IRC_AUTH_HOOK_TOKEN: "short" })).toThrow(
      "IRC_AUTH_HOOK_TOKEN must be at least 32 bytes",
    );
    expect(() => config({ ...hook, IRC_AUTH_HOOK_TOKEN: `${hookToken} with spaces` })).toThrow(
      "printable ASCII",
    );
  });

  test("the hook needs its token, its organisation and the credential secret together", () => {
    expect(() => config({ ...hook, IRC_AUTH_ORGANISATION: "" })).toThrow(
      "Set IRC_AUTH_HOOK_TOKEN and IRC_AUTH_ORGANISATION together",
    );
    expect(() => config({ ...hook, IRC_AUTH_HOOK_TOKEN: "" })).toThrow(
      "Set IRC_AUTH_HOOK_TOKEN and IRC_AUTH_ORGANISATION together",
    );
    expect(() => config({ ...hook, IRC_CREDENTIAL_SECRET: "" })).toThrow(
      "The IRC auth hook needs IRC_CREDENTIAL_SECRET",
    );
    for (const organisation of ["local", ":local", "dev:", "okta:example"])
      expect(() => config({ ...hook, IRC_AUTH_ORGANISATION: organisation })).toThrow(
        "IRC_AUTH_ORGANISATION must be <provider>:<organisation>",
      );
    expect(() => config({ ...hook, IRC_AUTH_ORGANISATION: "entra:" + userId })).toThrow(
      "IRC_AUTH_ORGANISATION must name a provider in AUTH_PROVIDERS",
    );
    const entra = {
      ...hook,
      AUTH_PROVIDERS: "entra",
      ENTRA_TENANT_ID: userId,
      ENTRA_CLIENT_ID: userId,
      ENTRA_AUDIENCE: userId,
    };
    expect(
      config({ ...entra, IRC_AUTH_ORGANISATION: `entra:${userId.toUpperCase()}` }).irc.hook,
    ).toMatchObject({ provider: "entra", externalOrganisation: userId });
    expect(() => config({ ...entra, IRC_AUTH_ORGANISATION: "entra:example" })).toThrow(
      "Entra tenant by its UUID",
    );
  });
});

test("the credential route is documented and the hook is not, and neither is an MCP tool", () => {
  const document = OpenApi.fromApi(documentedApi) as { paths: Record<string, unknown> };
  expect(Object.keys(document.paths)).toContain("/api/v1/me/irc-credential");
  expect(Object.keys(document.paths).some((path) => path.includes("/irc/"))).toBe(false);
  expect(Object.keys(mcpTools).filter((name) => /irc|credential/iu.test(name))).toEqual([]);
});

describe.skipIf(!testDatabase)("IRC sign-in over HTTP", () => {
  let database: Awaited<ReturnType<typeof isolatedDatabase>>;
  let tenant: Awaited<ReturnType<typeof entraTenant>>;
  let api: Awaited<ReturnType<typeof startApi>>;
  let bare: Awaited<ReturnType<typeof startApi>>;
  let credentialsOnly: Awaited<ReturnType<typeof startApi>>;

  beforeAll(async () => {
    database = await isolatedDatabase("irc");
    tenant = await entraTenant();
    api = await startApi(
      database.url,
      {
        ...tenant.env,
        IRC_CREDENTIAL_SECRET: Redacted.value(secret),
        IRC_AUTH_HOOK_TOKEN: hookToken,
        // The tenant in upper case, as an operator might paste it.
        IRC_AUTH_ORGANISATION: `entra:${tenant.tenant.toUpperCase()}`,
      },
      tenant.fetchRemote,
    );
    bare = await startApi(database.url, tenant.env, tenant.fetchRemote);
    credentialsOnly = await startApi(
      database.url,
      { ...tenant.env, IRC_CREDENTIAL_SECRET: Redacted.value(secret) },
      tenant.fetchRemote,
    );
  });
  afterAll(async () => {
    await api?.close();
    await bare?.close();
    await credentialsOnly?.close();
    await database?.drop();
  });

  const person = async (claims: Record<string, unknown>, oid = crypto.randomUUID()) => {
    const token = await tenant.sign(oid, claims);
    const me = await api.call("/api/v1/me", token);
    expect(me.status).toBe(200);
    return { token, oid, ...((await me.json()) as { userId: string; username: string }) };
  };
  const credential = async (token: string, server = api) => {
    const response = await server.call("/api/v1/me/irc-credential", token, "POST");
    return { status: response.status, response, body: await response.json() };
  };
  const hook = async (body: unknown, token = hookToken, server = api) => {
    const response = await server.call("/api/v1/irc/authenticate", token, "POST", body);
    return { status: response.status, body: await response.text() };
  };
  const raw = (text: string) =>
    fetch(`${api.origin}/api/v1/irc/authenticate`, {
      method: "POST",
      headers: { Authorization: `Bearer ${hookToken}`, "Content-Type": "application/json" },
      body: text,
    });
  const yes = (accountName: string) => ({
    status: 200,
    body: JSON.stringify({ success: true, accountName }),
  });
  const no = { status: 200, body: JSON.stringify({ success: false }) };

  test("a person gets a credential for their account, and the hook accepts it", async () => {
    const corin = await person({ given_name: "Corin", family_name: "Tarrowmere" });
    expect(corin.username).toBe("corin");
    const before = Date.now();
    const issued = await credential(corin.token);
    expect(issued.status).toBe(200);
    expect(issued.response.headers.get("cache-control")).toBe("private, no-store");
    expect(Object.keys(issued.body).sort()).toEqual(["account", "expiresAt", "password"]);
    expect(issued.body.account).toBe("corin");
    expect(issued.body.password).toMatch(/^sirc1\./u);
    expect(issued.body.password.length).toBeLessThanOrEqual(200);
    expect(issued.body.expiresAt).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/u);
    const expires = Date.parse(issued.body.expiresAt);
    expect(expires).toBeGreaterThanOrEqual(Math.floor(before / 1000) * 1000 + 599_000);
    expect(expires).toBeLessThanOrEqual(Date.now() + 600_000);

    const good = { accountName: "corin", passphrase: issued.body.password };
    expect(await hook(good)).toEqual(yes("corin"));
    // Reusable until it expires; case and a device suffix are ignored; Ergo's other fields too.
    expect(await hook(good)).toEqual(yes("corin"));
    expect(await hook({ ...good, accountName: "CORIN@laptop-1" })).toEqual(yes("corin"));
    expect(
      await hook({
        ...good,
        certfp: "",
        peerCerts: ["-----BEGIN CERTIFICATE-----"],
        ip: "192.0.2.10",
        oauth2: { token: "ignored" },
        cookies: { session: "ignored" },
      }),
    ).toEqual(yes("corin"));
  });

  test("expired, tampered and foreign credentials are refused", async () => {
    const tamsin = await person({ given_name: "Tamsin", family_name: "Quarrie" });
    const expired = mintIrcCredential(secret, tamsin.userId, Date.now() - 601_000).password;
    expect(await hook({ accountName: "tamsin", passphrase: expired })).toEqual(no);
    const { body } = await credential(tamsin.token);
    const tampered = `${body.password.slice(0, -2)}${body.password.endsWith("AA") ? "BB" : "AA"}`;
    expect(await hook({ accountName: "tamsin", passphrase: tampered })).toEqual(no);
    const foreign = mintIrcCredential(
      Redacted.make("another server's credential secret, 32 bytes"),
      tamsin.userId,
    ).password;
    expect(await hook({ accountName: "tamsin", passphrase: foreign })).toEqual(no);
  });

  test("the account name must be the credential's own account", async () => {
    const bryony = await person({ given_name: "Bryony", family_name: "Lisle" });
    const other = await person({ given_name: "Merrin", family_name: "Oakes" });
    const { body } = await credential(bryony.token);
    expect(await hook({ accountName: other.username, passphrase: body.password })).toEqual(no);
    expect(await hook({ accountName: "bryonylisle", passphrase: body.password })).toEqual(no);
    expect(await hook({ passphrase: body.password })).toEqual(no);
    expect(await hook({ accountName: "@bryony", passphrase: body.password })).toEqual(no);
    expect(await hook({ accountName: "bryony", passphrase: body.password })).toEqual(yes("bryony"));
  });

  test("a deactivated person's credential is refused", async () => {
    const jory = await person({ given_name: "Jory", family_name: "Pellow" });
    const { body } = await credential(jory.token);
    expect(await hook({ accountName: "jory", passphrase: body.password })).toEqual(yes("jory"));
    await database.client.query("UPDATE users SET active = false WHERE id = $1", [jory.userId]);
    expect(await hook({ accountName: "jory", passphrase: body.password })).toEqual(no);
    // And they can no longer obtain one.
    expect((await credential(jory.token)).status).toBe(403);
  });

  test("another organisation's user is refused, even with this server's secret", async () => {
    const [{ id: org }] = (
      await database.client.query(
        "INSERT INTO organisations (provider, external_id) VALUES ('dev', 'elsewhere') RETURNING id",
      )
    ).rows;
    const [{ id }] = (
      await database.client.query(
        "INSERT INTO users (organisation_id, subject, display_name, username) VALUES ($1, 'ext@example.com', 'Elsewhere', 'mallory') RETURNING id",
        [org],
      )
    ).rows;
    const { password } = mintIrcCredential(secret, id);
    expect(await hook({ accountName: "mallory", passphrase: password })).toEqual(no);
    // An unknown user too.
    const ghost = mintIrcCredential(secret, crypto.randomUUID()).password;
    expect(await hook({ accountName: "ghost", passphrase: ghost })).toEqual(no);
  });

  test("an agent's account is its IRC nick, and an agent without one gets 409", async () => {
    const owner = await person({ given_name: "Owner" });
    const [{ organisation_id: org }] = (
      await database.client.query("SELECT organisation_id FROM users WHERE id = $1", [owner.userId])
    ).rows;
    const agent = async (name: string, nick: string | null) => {
      const oid = crypto.randomUUID();
      await database.client.query(
        `INSERT INTO users (organisation_id, subject, display_name, agent_rank, agent_fleet, agent_model, registered_by_id, agent_irc_nick)
         VALUES ($1, $2, $3, 'crew', 'alpha', 'example-model', $4, $5)`,
        [org, oid, name, owner.userId, nick],
      );
      return tenant.sign(oid);
    };
    const orion = await credential(await agent("Orion", "Orion-1"));
    expect(orion.status).toBe(200);
    expect(orion.body.account).toBe("Orion-1");
    expect(await hook({ accountName: "orion-1@vm", passphrase: orion.body.password })).toEqual(
      yes("Orion-1"),
    );
    const nickless = await credential(await agent("Vega", null));
    expect(nickless.status).toBe(409);
    expect(nickless.body.detail).toBe(
      "Agent 'Vega' has no IRC nick. Ask an administrator to set one.",
    );
  });

  test("anything but a good credential answers success false with 200", async () => {
    const kj = await person({ given_name: "Demelza", family_name: "Rowe" });
    const { body } = await credential(kj.token);
    // A certificate alone is reserved for agent key credentials.
    expect(await hook({ accountName: "demelza", certfp: "ab".repeat(32) })).toEqual(no);
    expect(
      await hook({ accountName: "demelza", certfp: "ab".repeat(32), passphrase: body.password }),
    ).toEqual(yes("demelza"));
    // Local accounts' passwords and empty input are left to Ergo.
    expect(await hook({ accountName: "demelza", passphrase: "a local password" })).toEqual(no);
    expect(await hook({ accountName: "demelza", passphrase: "" })).toEqual(no);
    expect(await hook({})).toEqual(no);
    // A body over 256 KiB is not read.
    const huge = JSON.stringify({
      accountName: "demelza",
      passphrase: body.password,
      peerCerts: ["a".repeat(300_000)],
    });
    for (const text of ["", "not json", "[]", "null", '"sirc1."', '{"passphrase": 7}', huge])
      expect([text.slice(0, 40), (await raw(text)).status, await (await raw(text)).text()]).toEqual(
        [text.slice(0, 40), 200, JSON.stringify({ success: false })],
      );
  });

  test("a wrong or missing hook token is refused before anything is read", async () => {
    const response = await api.call("/api/v1/irc/authenticate", undefined, "POST", {});
    expect(response.status).toBe(401);
    expect(response.headers.get("www-authenticate")).toBe("Bearer");
    expect((await hook({}, `${hookToken}x`)).status).toBe(401);
    expect((await hook({}, hookToken.slice(0, -1))).status).toBe(401);
    // A user's bearer token is not the hook token.
    const user = await person({ given_name: "Morwen" });
    expect((await hook({}, user.token)).status).toBe(401);
  });

  test("the routes are absent when unconfigured", async () => {
    const user = await person({ given_name: "Elowen" });
    const off = await credential(user.token, bare);
    expect(off.status).toBe(404);
    expect(off.body.detail).toBe("This server does not issue IRC credentials.");
    expect((await hook({}, hookToken, bare)).status).toBe(404);
    // A credential secret alone issues credentials but serves no hook.
    expect((await credential(user.token, credentialsOnly)).status).toBe(200);
    expect((await hook({}, hookToken, credentialsOnly)).status).toBe(404);
    // A credential needs a signed-in user.
    expect((await api.call("/api/v1/me/irc-credential", undefined, "POST")).status).toBe(401);
  });
});
