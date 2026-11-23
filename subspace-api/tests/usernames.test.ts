import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtemp, readdir, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { Effect } from "effect";
import pg from "pg";
import { config, reservedUsernames } from "../src/config";
import { Database, databaseLayer } from "../src/db";
import { localMember, provisionMember } from "../src/identity";
import {
  allocatedUsername,
  foldName,
  isReservedUsername,
  backfillUsernames,
  nameParts,
  setUsername,
  usernameCandidates,
} from "../src/usernames";
import {
  applyMigrations,
  entraTenant,
  isolatedDatabase,
  startApi,
  testDatabase,
} from "./entra-stack";

const first = (count: number, parts: { first: string; surname: string }) => {
  const names: string[] = [];
  for (const name of usernameCandidates(parts)) {
    names.push(name);
    if (names.length === count) break;
  }
  return names;
};

describe("the username rule", () => {
  test("names fold to lower-case ASCII letters and digits", () => {
    expect(foldName("Zoë")).toBe("zoe");
    expect(foldName("José María")).toBe("josemaria");
    expect(foldName("Łukasz")).toBe("lukasz");
    expect(foldName("Søren Ægir")).toBe("sorenaegir");
    expect(foldName("Straße")).toBe("strasse");
    expect(foldName("Þóra Ðoe")).toBe("thoradoe");
    expect(foldName("Ｊｏ")).toBe("jo");
    expect(foldName("O'Brien-Smith_2")).toBe("obriensmith2");
    expect(foldName("王小明")).toBe("");
    expect(foldName("İlkay")).toBe("ilkay");
  });

  test("Tamsin Quarrie is tamsin, then tamsinq, tamsinqu and so on, then tamsin2", () => {
    expect(first(11, nameParts({ givenName: "Tamsin", familyName: "Quarrie" }))).toEqual([
      "tamsin",
      "tamsinq",
      "tamsinqu",
      "tamsinqua",
      "tamsinquar",
      "tamsinquarr",
      "tamsinquarri",
      "tamsinquarrie",
      "tamsin2",
      "tamsin3",
      "tamsin4",
    ]);
  });

  test("the first and family names come from claims, then the name, then the e-mail", () => {
    expect(nameParts({ givenName: "Tamsin", familyName: "Quarrie", name: "Countess" })).toEqual({
      first: "tamsin",
      surname: "quarrie",
    });
    // Without a family name, the surname comes from the display name.
    expect(nameParts({ givenName: "Tamsin", name: "Tamsin Quarrie" })).toEqual({
      first: "tamsin",
      surname: "quarrie",
    });
    expect(nameParts({ name: "Tamsin Kestle Quarrie" })).toEqual({
      first: "tamsin",
      surname: "kestlequarrie",
    });
    expect(nameParts({ name: "Quarrie, Tamsin" })).toEqual({ first: "tamsin", surname: "quarrie" });
    // A given name with no ASCII letters falls back to the name, then the e-mail.
    expect(
      nameParts({ givenName: "小明", name: "王小明", email: "xiaoming.wang@example.com" }),
    ).toEqual({ first: "xiaoming", surname: "wang" });
    expect(nameParts({ email: "bryony_lisle+work@example.com" })).toEqual({
      first: "bryony",
      surname: "lisle",
    });
    expect(nameParts({ name: "王小明" })).toEqual({ first: "user", surname: "" });
    expect(nameParts({})).toEqual({ first: "user", surname: "" });
  });

  test("a username starts with a letter and has at most 32 characters", () => {
    expect(nameParts({ givenName: "2Bea", familyName: "Example" }).first).toBe("bea");
    expect(nameParts({ name: "007 Bond" })).toEqual({ first: "bond", surname: "" });
    const long = "a".repeat(40);
    const names = first(3, nameParts({ givenName: long, familyName: "Smith" }));
    expect(names).toEqual(["a".repeat(32), `${"a".repeat(31)}2`, `${"a".repeat(31)}3`]);
    const near = first(4, nameParts({ givenName: "b".repeat(30), familyName: "Smith" }));
    expect(near).toEqual([
      "b".repeat(30),
      `${"b".repeat(30)}s`,
      `${"b".repeat(30)}sm`,
      `${"b".repeat(30)}2`,
    ]);
    for (const name of [...names, ...near]) expect(allocatedUsername.test(name)).toBe(true);
  });

  test("IRC services, guest names and operator names are reserved", () => {
    const operators = new Set(["root"]);
    for (const name of [
      "nickserv",
      "NickServ",
      "chanserv",
      "operserv",
      "hostserv",
      "histserv",
      "botserv",
      "memoserv",
      "global",
      "guest",
      "Guest-12",
      "guest-",
      "root",
      "ROOT",
    ])
      expect([name, isReservedUsername(name, operators)]).toEqual([name, true]);
    for (const name of ["guest1", "guests", "nickservs", "rooted"])
      expect([name, isReservedUsername(name, operators)]).toEqual([name, false]);
  });

  test("RESERVED_USERNAMES is a comma-separated list of names", () => {
    expect([...reservedUsernames({ RESERVED_USERNAMES: " Root, ops-team ,," })]).toEqual([
      "root",
      "ops-team",
    ]);
    expect([...reservedUsernames({})]).toEqual([]);
    expect(() => reservedUsernames({ RESERVED_USERNAMES: "root,two words" })).toThrow(
      "RESERVED_USERNAMES",
    );
    expect(() => reservedUsernames({ RESERVED_USERNAMES: "zoë" })).toThrow("RESERVED_USERNAMES");
    // Existing IRC accounts may have any name Ergo accepts.
    expect([...reservedUsernames({ RESERVED_USERNAMES: "1st,[away]" })]).toEqual(["1st", "[away]"]);
    expect([
      ...config({
        DATABASE_URL: "postgresql://subspace@127.0.0.1/subspace",
        WORKOS_CLIENT_ID: "client_example",
        RESERVED_USERNAMES: "root",
      }).reservedUsernames,
    ]).toEqual(["root"]);
  });
});

test("RESERVED_USERNAMES_FILE lists names by line, comma or space, with comments", async () => {
  const folder = await mkdtemp(join(tmpdir(), "subspace-reserved-"));
  try {
    const file = join(folder, "accounts");
    await writeFile(
      file,
      "# Accounts on the IRC server\nOperator\nTamsin_Q, kerra\n  # indented comment\nlowen  bryn\n\n",
    );
    expect([
      ...reservedUsernames({ RESERVED_USERNAMES: "root", RESERVED_USERNAMES_FILE: file }),
    ]).toEqual(["root", "operator", "tamsin_q", "kerra", "lowen", "bryn"]);
    expect(() => reservedUsernames({ RESERVED_USERNAMES_FILE: join(folder, "missing") })).toThrow(
      "RESERVED_USERNAMES_FILE must name a readable file",
    );
  } finally {
    await rm(folder, { recursive: true });
  }
});

describe.skipIf(!testDatabase)("usernames on registration", () => {
  let database: Awaited<ReturnType<typeof isolatedDatabase>>;
  let tenant: Awaited<ReturnType<typeof entraTenant>>;
  let api: Awaited<ReturnType<typeof startApi>>;
  const adminRole = "Subspace.Admin";

  beforeAll(async () => {
    database = await isolatedDatabase("usernames");
    tenant = await entraTenant();
    api = await startApi(
      database.url,
      { ...tenant.env, ENTRA_ADMIN_ROLE: adminRole, RESERVED_USERNAMES: "root,ops" },
      tenant.fetchRemote,
    );
  });
  afterAll(async () => {
    await api?.close();
    await database?.drop();
  });

  const signIn = async (claims: Record<string, unknown>, oid = crypto.randomUUID()) => {
    const response = await api.call("/api/v1/me", await tenant.sign(oid, claims));
    expect(response.status).toBe(200);
    return (await response.json()) as { userId: string; username: string | null };
  };
  const stored = async (userId: string) =>
    (await database.client.query("SELECT username FROM users WHERE id = $1", [userId])).rows[0]
      .username as string | null;

  test("a person registered as Wenna Nethercott is wenna, and the next Wenna Nethercott is wennan", async () => {
    const wenna = await signIn({
      given_name: "Wenna",
      family_name: "Nethercott",
      name: "Wenna Nethercott",
    });
    expect(wenna.username).toBe("wenna");
    expect(await stored(wenna.userId)).toBe("wenna");
    const second = await signIn({ given_name: "Wenna", family_name: "Nethercott" });
    expect(second.username).toBe("wennan");
    const third = await signIn({ given_name: "WENNA", family_name: "Nevillow" });
    expect(third.username).toBe("wennane");
    // The username never changes by itself, whatever later tokens say.
    const again = await api.call(
      "/api/v1/me",
      await tenant.sign(crypto.randomUUID(), { given_name: "Other" }),
    );
    expect(again.status).toBe(200);
    expect(await stored(wenna.userId)).toBe("wenna");
  });

  test("concurrent registrations of people with the same first name get different usernames", async () => {
    const surnames = ["Quarrie", "Brackwater", "Kestle", "Lisle", "Tarrowmere", "Nethercott"];
    // Hold the organisation row while every registration starts, then release them together, so
    // they race for the same name rather than arriving one after another. Without the
    // registration's own lock on that row, they all choose tamsin and all but one fail.
    await signIn({ name: "Organisation Seed" });
    await database.client.query("BEGIN");
    const held = await database.client.query(
      "SELECT id FROM organisations WHERE provider = 'entra' AND external_id = $1 FOR UPDATE",
      [tenant.tenant],
    );
    expect(held.rowCount).toBe(1);
    const pending = Promise.all(
      surnames.map((surname) => signIn({ given_name: "Tamsin", family_name: surname })),
    );
    await Bun.sleep(300);
    await database.client.query("COMMIT");
    const people = await pending;
    const names = people.map((person) => person.username!);
    expect(new Set(names).size).toBe(surnames.length);
    expect(names.filter((name) => name === "tamsin")).toHaveLength(1);
    // Every other one is tamsin followed by the start of their own surname.
    names.forEach((name, index) => {
      if (name !== "tamsin")
        expect(`tamsin${surnames[index]!.toLowerCase()}`.startsWith(name)).toBe(true);
    });
  });

  test("a person never takes an agent's IRC nick, ignoring case", async () => {
    const owner = await signIn({ name: "Owner Person" });
    const [{ organisation_id: org }] = (
      await database.client.query("SELECT organisation_id FROM users WHERE id = $1", [owner.userId])
    ).rows;
    await database.client.query(
      `INSERT INTO users (organisation_id, subject, display_name, agent_rank, agent_fleet, agent_model, registered_by_id, agent_irc_nick)
       VALUES ($1, $2, 'Bryony-agent', 'crew', 'alpha', 'example-model', $3, 'Bryony')`,
      [org, crypto.randomUUID(), owner.userId],
    );
    const bryony = await signIn({ given_name: "Bryony", family_name: "Lisle" });
    expect(bryony.username).toBe("bryonyl");
  });

  test("reserved names are skipped: IRC services, guest and the operator's names", async () => {
    expect((await signIn({ given_name: "Global", family_name: "Smith" })).username).toBe("globals");
    expect((await signIn({ given_name: "NickServ", family_name: "Jones" })).username).toBe(
      "nickservj",
    );
    expect((await signIn({ given_name: "Guest", family_name: "Star" })).username).toBe("guests");
    expect((await signIn({ given_name: "Root", family_name: "Beer" })).username).toBe("rootb");
    expect((await signIn({ given_name: "Ops" })).username).toBe("ops2");
  });

  test("non-ASCII names fold, and missing claims fall back to the name and the e-mail", async () => {
    expect((await signIn({ given_name: "Zoë", family_name: "Ålvik" })).username).toBe("zoe");
    expect((await signIn({ name: "Łukasz Żółw" })).username).toBe("lukasz");
    expect((await signIn({ name: "Fernsby, Émilie" })).username).toBe("emilie");
    expect((await signIn({ name: "王小明", email: "xiaoming.wang@example.com" })).username).toBe(
      "xiaoming",
    );
    expect((await signIn({ preferred_username: "merrin.lamarr@example.com" })).username).toBe(
      "merrin",
    );
    const nameless = await signIn({ name: "王小明" });
    expect(nameless.username).toBe("user");
    expect((await signIn({})).username).toBe("user2");
  });

  test("people and agents report their usernames where the API lists people", async () => {
    const admin = await tenant.sign(crypto.randomUUID(), {
      given_name: "Kerensa",
      family_name: "Doble",
      roles: [adminRole],
    });
    const me = await (await api.call("/api/v1/me", admin)).json();
    expect(me.username).toBe("kerensa");
    const users = await (await api.call("/api/v1/users?limit=200", admin)).json();
    expect(users.items).toContainEqual(
      expect.objectContaining({ id: me.userId, username: "kerensa", kind: "human" }),
    );
    expect(users.items).toContainEqual(
      expect.objectContaining({ username: "Bryony", kind: "agent" }),
    );
  });

  test("an agent's username follows its IRC nick, and a nick cannot take a person's username", async () => {
    const admin = await tenant.sign(crypto.randomUUID(), {
      given_name: "Morwen",
      family_name: "Tregear",
      roles: [adminRole],
    });
    expect((await (await api.call("/api/v1/me", admin)).json()).username).toBe("morwen");
    const fleet = await api.call("/api/v1/fleets", admin, "POST", {
      name: "Example fleet",
      slug: "username-fleet",
      ircHost: "irc.example.com",
      ircPort: 6697,
      ircTls: true,
    });
    expect(fleet.status).toBe(201);
    const register = (ircNick: string | null, name = "Vega") =>
      api.call("/api/v1/agents", admin, "POST", {
        name,
        rank: "rearAdmiral",
        fleet: "username-fleet",
        defaultModel: "example-model",
        serviceTokenClientId: crypto.randomUUID(),
        ircNick,
      });
    const taken = await register("MORWEN");
    expect(taken.status).toBe(409);
    const reserved = await register("ChanServ");
    expect(reserved.status).toBe(400);
    expect((await reserved.json()).errors).toEqual({
      ircNick: ["This IRC nick is reserved. Choose another."],
    });
    expect((await register("Guest-7")).status).toBe(400);
    const created = await register("Vega-1");
    expect(created.status).toBe(201);
    const agent = await created.json();
    expect(await stored(agent.userId)).toBe("Vega-1");
    // A person now registering as Vega is not given the agent's name.
    expect((await signIn({ given_name: "Vega", family_name: "Lyra" })).username).toBe("vega");
    expect((await signIn({ given_name: "Vega1" })).username).toBe("vega1");

    const edit = await api.call(`/api/v1/agents/${agent.userId}`, admin, "PUT", {
      name: "Vega",
      rank: "rearAdmiral",
      fleet: "username-fleet",
      defaultModel: "example-model",
      serviceTokenClientId: agent.serviceTokenClientId,
      ircNick: "vega-two",
      version: agent.version,
    });
    expect(edit.status).toBe(200);
    expect(await stored(agent.userId)).toBe("vega-two");
    // The trigger keeps the two in step even for a writer that sets only one of them.
    await database.client.query("UPDATE users SET agent_irc_nick = 'Vega-3' WHERE id = $1", [
      agent.userId,
    ]);
    expect(await stored(agent.userId)).toBe("Vega-3");
    await database.client.query("UPDATE users SET username = 'someone' WHERE id = $1", [
      agent.userId,
    ]);
    expect(await stored(agent.userId)).toBe("Vega-3");
    await database.client.query("UPDATE users SET agent_irc_nick = NULL WHERE id = $1", [
      agent.userId,
    ]);
    expect(await stored(agent.userId)).toBeNull();
  });

  test("a username that was issued is never given to anyone else", async () => {
    const admin = await tenant.sign(crypto.randomUUID(), {
      given_name: "Senara",
      family_name: "Polglaze",
      roles: [adminRole],
    });
    expect((await api.call("/api/v1/me", admin)).status).toBe(200);
    const fleet = await api.call("/api/v1/fleets", admin, "POST", {
      name: "Retirement fleet",
      slug: "retirement-fleet",
      ircHost: "irc.example.com",
      ircPort: 6697,
      ircTls: true,
    });
    expect(fleet.status).toBe(201);
    const fields = (name: string, ircNick: string | null, serviceTokenClientId: string) => ({
      name,
      rank: "rearAdmiral",
      fleet: "retirement-fleet",
      defaultModel: "example-model",
      serviceTokenClientId,
      ircNick,
    });
    const created = await api.call(
      "/api/v1/agents",
      admin,
      "POST",
      fields("Pellam", "Pellam", crypto.randomUUID()),
    );
    expect(created.status).toBe(201);
    let agent = await created.json();
    const rename = async (ircNick: string | null) => {
      const response = await api.call(`/api/v1/agents/${agent.userId}`, admin, "PUT", {
        ...fields("Pellam", ircNick, agent.serviceTokenClientId),
        version: agent.version,
      });
      expect(response.status).toBe(200);
      agent = await response.json();
    };
    const retired = async () =>
      (
        await database.client.query(
          "SELECT name FROM retired_usernames WHERE user_id = $1 ORDER BY name",
          [agent.userId],
        )
      ).rows.map((row) => row.name);

    // A change of case keeps the same name; a new nick retires the old one.
    await rename("PELLAM");
    expect(await retired()).toEqual([]);
    await rename("Pellam-two");
    expect(await retired()).toEqual(["pellam"]);
    // A new person named Pellam does not get pellam.
    expect((await signIn({ given_name: "Pellam", family_name: "Rook" })).username).toBe("pellamr");
    // Nor does another agent.
    const another = await api.call(
      "/api/v1/agents",
      admin,
      "POST",
      fields("Pellam-copy", "pellam", crypto.randomUUID()),
    );
    expect(another.status).toBe(409);
    // The agent that held it may take it back, and clearing its nick retires the other.
    await rename("Pellam");
    expect(await stored(agent.userId)).toBe("Pellam");
    await rename(null);
    expect(await stored(agent.userId)).toBeNull();
    expect(await retired()).toEqual(["pellam", "pellam-two"]);
    // The database refuses a retired name to any other record, however it is written.
    const [{ organisation_id: org }] = (
      await database.client.query("SELECT organisation_id FROM users WHERE id = $1", [agent.userId])
    ).rows;
    await expect(
      database.client.query(
        "INSERT INTO users (organisation_id, subject, display_name, username) VALUES ($1, $2, 'Copy', 'PELLAM-TWO')",
        [org, crypto.randomUUID()],
      ),
    ).rejects.toThrow("retired");
  });

  test("names compare in ASCII lower case whatever the database locale", async () => {
    const indexes = (
      await database.client.query(
        "SELECT indexname, indexdef FROM pg_indexes WHERE indexname IN ('users_username', 'user_agent_irc_nick') ORDER BY indexname",
      )
    ).rows;
    expect(indexes).toHaveLength(2);
    for (const { indexdef } of indexes) expect(indexdef).toContain('COLLATE "C"');
  });

  test("the database refuses a second username that differs only in case", async () => {
    const person = await signIn({ given_name: "Demelza" });
    const [{ organisation_id: org }] = (
      await database.client.query("SELECT organisation_id FROM users WHERE id = $1", [
        person.userId,
      ])
    ).rows;
    await expect(
      database.client.query(
        "INSERT INTO users (organisation_id, subject, display_name, username) VALUES ($1, $2, 'Copy', 'DEMELZA')",
        [org, crypto.randomUUID()],
      ),
    ).rejects.toThrow("users_username");
  });

  test("a person without a username is given one on their next request", async () => {
    // As an older API instance adds a person: no username. Clearing one would retire it instead.
    const oid = crypto.randomUUID();
    const [{ id: userId }] = (
      await database.client.query(
        `INSERT INTO users (organisation_id, subject, display_name)
         SELECT id, $2, 'Elowen Trevail' FROM organisations WHERE provider = 'entra' AND external_id = $1
         RETURNING id`,
        [tenant.tenant, oid],
      )
    ).rows;
    const person = { userId: userId as string };
    const responses = await Promise.all(
      Array.from({ length: 4 }, async () =>
        (await api.call("/api/v1/me", await tenant.sign(oid))).json(),
      ),
    );
    expect(new Set(responses.map((body) => body.username))).toEqual(new Set(["elowen"]));
    expect(await stored(person.userId)).toBe("elowen");
    expect(
      Number(
        (
          await database.client.query(
            "SELECT count(*) AS n FROM audit_logs WHERE action = 'usernameAllocated' AND entity_id = $1",
            [person.userId],
          )
        ).rows[0].n,
      ),
    ).toBe(1);
  });

  test("the membership command allocates a username once and keeps it", async () => {
    const identity = {
      provider: "entra" as const,
      externalOrganisation: tenant.tenant,
      subject: crypto.randomUUID(),
    };
    const provision = (name: string, active = true) =>
      Effect.runPromise(
        provisionMember(
          identity,
          { displayName: name, email: null },
          "member",
          active,
          new Set(["lowenna"]),
        ).pipe(Effect.provide(databaseLayer(database.url, 2))),
      );
    const saved = await provision("Lowenna Hext");
    expect(saved.username).toBe("lowennah");
    expect((await provision("Someone Else")).username).toBe("lowennah");
    expect((await provision("Someone Else", false)).username).toBe("lowennah");
  });

  test("an administrator can set a person's username, but not to another's or a retired name", async () => {
    const first = await signIn({ given_name: "Kerra", family_name: "Jago" });
    const second = await signIn({ given_name: "Kerra", family_name: "Polkinghorne" });
    expect([first.username, second.username]).toEqual(["kerra", "kerrap"]);
    const subjectOf = async (userId: string) =>
      (await database.client.query("SELECT subject FROM users WHERE id = $1", [userId])).rows[0]
        .subject as string;
    const set = async (userId: string, name: string, subject?: string) => {
      const who = subject ?? (await subjectOf(userId));
      const result = await Effect.runPromise(
        Effect.result(
          Effect.flatMap(Database, (db) =>
            setUsername(
              db,
              {
                provider: "entra",
                externalOrganisation: tenant.tenant,
                subject: who,
              },
              name,
            ),
          ).pipe(Effect.provide(databaseLayer(database.url, 2))),
        ),
      );
      return "failure" in result && result.failure
        ? (result.failure as Error).message
        : (result as { success: { username: string } }).success.username;
    };
    // A reserved name can be given deliberately, and the old name is retired.
    expect(await set(first.userId, "Root")).toBe("Root");
    expect(await stored(first.userId)).toBe("Root");
    expect((await signIn({ given_name: "Kerra", family_name: "Jory" })).username).toBe("kerraj");
    expect(await set(second.userId, "ROOT")).toBe("Another member or agent has that name.");
    expect(await set(second.userId, "kerra")).toBe(
      "That name was issued to someone else before and stays retired.",
    );
    expect(await set(first.userId, "kerra")).toBe("kerra");
    expect(await set(second.userId, "two words")).toContain("Use a letter");
    for (const name of ["NickServ", "global", "guest", "Guest-7"])
      expect(await set(second.userId, name)).toBe(
        "That name belongs to the IRC server's services or guests.",
      );
    expect(await set(second.userId, "x", crypto.randomUUID())).toContain("No member");
    const [{ subject: agentSubject }] = (
      await database.client.query(
        "SELECT subject FROM users WHERE agent_rank IS NOT NULL AND agent_irc_nick IS NOT NULL LIMIT 1",
      )
    ).rows;
    expect(await set(second.userId, "agentname", agentSubject)).toContain("is an agent");
    const audit = (
      await database.client.query(
        `SELECT "actorType" AS actor, source, changes FROM audit_logs WHERE action = 'usernameSet' AND entity_id = $1 ORDER BY occurred_at`,
        [first.userId],
      )
    ).rows;
    expect(audit).toEqual([
      { actor: "integration", source: "cli", changes: { from: "kerra", to: "Root" } },
      { actor: "integration", source: "cli", changes: { from: "Root", to: "kerra" } },
    ]);

    // The command itself.
    const command = (...args: string[]) =>
      Bun.spawnSync([process.execPath, "scripts/username.ts", ...args], {
        cwd: fileURLToPath(new URL("..", import.meta.url)),
        env: { ...process.env, DATABASE_URL: database.url },
        stdout: "pipe",
        stderr: "pipe",
      });
    const saved = command("entra", tenant.tenant, await subjectOf(second.userId), "Polkinghorne_K");
    expect([saved.exitCode, saved.stdout.toString().trim()]).toEqual([
      0,
      "Username changed from kerrap to Polkinghorne_K; kerrap is retired.",
    ]);
    const refused = command("entra", tenant.tenant, await subjectOf(second.userId), "kerra");
    expect([refused.exitCode, refused.stderr.toString().trim()]).toEqual([
      1,
      "Another member or agent has that name.",
    ]);
  });
});

describe.skipIf(!testDatabase)("usernames at development sign-in", () => {
  test("they come from the name given, else from the e-mail", async () => {
    const database = await isolatedDatabase("dev_usernames");
    try {
      const dev = { keyPath: "", channels: "#example", ircHost: "127.0.0.1", ircPort: 6667 };
      const signIn = (email: string, name?: string) =>
        Effect.runPromise(
          localMember(email, name ?? "Local Developer", "local", dev, new Set(["root"]), {
            name,
            email,
          }).pipe(Effect.provide(databaseLayer(database.url, 2))),
        );
      expect((await signIn("dev@example.com")).username).toBe("dev");
      expect((await signIn("member@example.com")).username).toBe("member");
      expect((await signIn("tamsin@example.com", "Tamsin Quarrie")).username).toBe("tamsin");
      expect((await signIn("root@example.com")).username).toBe("root2");
      // A second sign-in keeps the member and their username.
      expect((await signIn("dev@example.com", "Someone Else")).username).toBe("dev");
    } finally {
      await database.drop();
    }
  });
});

describe.skipIf(!testDatabase)("the username backfill", () => {
  test("it waits for an organisation in short batches without holding the users table", async () => {
    const database = await isolatedDatabase("username_backfill_locks");
    const blocker = new pg.Client(database.url);
    await blocker.connect();
    try {
      const { client } = database;
      const org = "00000000-0000-4000-8000-000000000011";
      const other = "00000000-0000-4000-8000-000000000012";
      await client.query(
        "INSERT INTO organisations (id, provider, external_id) VALUES ($1, 'dev', 'locks-one'), ($2, 'dev', 'locks-two')",
        [org, other],
      );
      const names = [
        "Tamsin Quarrie",
        "Tamsin Brackwater",
        "Kerra Jago",
        "Kerra Jory",
        "Bryn Tallack",
      ];
      for (const [index, name] of names.entries())
        await client.query(
          "INSERT INTO users (organisation_id, subject, display_name, created_at) VALUES ($1, $2, $3, $4)",
          [org, `person-${index}@example.com`, name, `2026-01-0${index + 1}T00:00:00Z`],
        );

      await blocker.query("BEGIN");
      await blocker.query("SELECT id FROM organisations WHERE id = $1 FOR UPDATE", [org]);
      const pending = Effect.runPromise(
        Effect.flatMap(Database, (db) => backfillUsernames(db, new Set(), 2)).pipe(
          Effect.provide(databaseLayer(database.url, 2)),
        ),
      );
      await Bun.sleep(300);
      // While the backfill waits for that organisation, the users table stays open to others.
      await client.query("SET statement_timeout = 2000");
      expect(Number((await client.query("SELECT count(*) AS n FROM users")).rows[0].n)).toBe(5);
      await client.query(
        "INSERT INTO users (organisation_id, subject, display_name, username) VALUES ($1, 'late@example.com', 'Late Person', 'late')",
        [other],
      );
      await client.query("RESET statement_timeout");
      expect(
        Number(
          (await client.query("SELECT count(*) AS n FROM users WHERE username IS NOT NULL")).rows[0]
            .n,
        ),
      ).toBe(1);
      await blocker.query("COMMIT");
      expect(await pending).toBe(5);
      expect(
        (
          await client.query(
            "SELECT username FROM users WHERE organisation_id = $1 ORDER BY created_at",
            [org],
          )
        ).rows.map((row) => row.username),
      ).toEqual(["tamsin", "tamsinb", "kerra", "kerraj", "bryn"]);
    } finally {
      await blocker.end();
      await database.drop();
    }
  });

  test("existing people get usernames oldest first, agents keep their nicks, and a rerun changes nothing", async () => {
    const database = await isolatedDatabase("username_backfill", false);
    try {
      const folder = fileURLToPath(new URL("../drizzle", import.meta.url));
      const directories = (await readdir(folder)).sort();
      const usernames = directories.find((name) => name.endsWith("_usernames"))!;
      expect(usernames).toBeDefined();
      const previous = await mkdtemp(join(tmpdir(), "subspace-drizzle-"));
      try {
        for (const name of directories.filter((name) => name < usernames))
          await symlink(join(folder, name), join(previous, name), "dir");
        await applyMigrations(database.url, previous);
      } finally {
        await rm(previous, { recursive: true });
      }

      const { client } = database;
      const org = "00000000-0000-4000-8000-000000000001";
      const other = "00000000-0000-4000-8000-000000000002";
      await client.query(
        "INSERT INTO organisations (id, provider, external_id) VALUES ($1, 'dev', 'one'), ($2, 'dev', 'two')",
        [org, other],
      );
      const person = async (
        organisation: string,
        name: string,
        email: string | null,
        createdAt: string,
        fields = "",
        values: unknown[] = [],
      ) =>
        (
          await client.query(
            `INSERT INTO users (organisation_id, subject, display_name, email, created_at${fields})
             VALUES ($1, $2, $3, $4, $5${values.map((_, index) => `, $${index + 6}`).join("")})
             RETURNING id`,
            [organisation, crypto.randomUUID(), name, email, createdAt, ...values],
          )
        ).rows[0].id as string;
      // Inserted newest first, so only the backfill's order decides who is tamsin.
      const byron = await person(org, "Tamsin Brackwater", null, "2026-03-01T00:00:00Z");
      const quarrie = await person(org, "Tamsin Quarrie", null, "2026-01-01T00:00:00Z");
      const otherAda = await person(other, "Tamsin Clone", null, "2026-04-01T00:00:00Z");
      const corin = await person(org, "Corin Tarrowmere", null, "2026-01-02T00:00:00Z");
      const root = await person(org, "Root Admin", null, "2026-01-03T00:00:00Z");
      const nameless = await person(
        org,
        "王小明",
        "xiaoming.wang@example.com",
        "2026-01-04T00:00:00Z",
      );
      const former = await person(org, "Former Person", null, "2026-01-05T00:00:00Z", ", active", [
        false,
      ]);
      const agent = await person(
        org,
        "Corin",
        null,
        "2025-12-01T00:00:00Z",
        ", agent_rank, agent_fleet, agent_model, registered_by_id, agent_irc_nick",
        ["crew", "alpha", "example-model", quarrie, "Corin"],
      );
      const nickless = await person(
        org,
        "Nickless",
        null,
        "2025-12-02T00:00:00Z",
        ", agent_rank, agent_fleet, agent_model, registered_by_id",
        ["crew", "alpha", "example-model", quarrie],
      );

      // The server's existing accounts, reserved in a file, as the cut-over steps describe.
      const reservedFolder = await mkdtemp(join(tmpdir(), "subspace-reserved-"));
      const reservedFile = join(reservedFolder, "accounts");
      await writeFile(reservedFile, "# Existing IRC accounts\nFormer\n");
      const migrate = () => {
        const run = Bun.spawnSync([process.execPath, "scripts/migrate.ts"], {
          cwd: fileURLToPath(new URL("..", import.meta.url)),
          env: {
            ...process.env,
            DATABASE_URL: database.url,
            RESERVED_USERNAMES: "root",
            RESERVED_USERNAMES_FILE: reservedFile,
          },
          stdout: "pipe",
          stderr: "pipe",
        });
        expect([run.exitCode, run.stderr.toString()]).toEqual([0, ""]);
      };
      migrate();
      const names = async () =>
        Object.fromEntries(
          (await client.query("SELECT id, username FROM users ORDER BY id")).rows.map((row) => [
            row.id,
            row.username,
          ]),
        );
      const result = await names();
      expect(result).toEqual({
        [quarrie]: "tamsin",
        [byron]: "tamsinb",
        [otherAda]: "tamsin",
        [corin]: "corint",
        [root]: "roota",
        [nameless]: "xiaoming",
        [former]: "formerp",
        [agent]: "Corin",
        [nickless]: null,
      });
      migrate();
      expect(await names()).toEqual(result);
      await rm(reservedFolder, { recursive: true });
    } finally {
      await database.drop();
    }
  });
});
