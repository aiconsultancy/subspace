import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtemp, readdir, rm, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { Effect } from "effect";
import { config, reservedUsernames } from "../src/config";
import { databaseLayer } from "../src/db";
import { localMember, provisionMember } from "../src/identity";
import {
  allocatedUsername,
  foldName,
  isReservedUsername,
  nameParts,
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
    expect(foldName("Ａｄａ")).toBe("ada");
    expect(foldName("O'Brien-Smith_2")).toBe("obriensmith2");
    expect(foldName("王小明")).toBe("");
    expect(foldName("İlkay")).toBe("ilkay");
  });

  test("Ada Lovelace is ada, then adal, adalo and so on, then ada2", () => {
    expect(first(12, nameParts({ givenName: "Ada", familyName: "Lovelace" }))).toEqual([
      "ada",
      "adal",
      "adalo",
      "adalov",
      "adalove",
      "adalovel",
      "adalovela",
      "adalovelac",
      "adalovelace",
      "ada2",
      "ada3",
      "ada4",
    ]);
  });

  test("the first and family names come from claims, then the name, then the e-mail", () => {
    expect(nameParts({ givenName: "Ada", familyName: "Lovelace", name: "Countess" })).toEqual({
      first: "ada",
      surname: "lovelace",
    });
    // Without a family name, the surname comes from the display name.
    expect(nameParts({ givenName: "Ada", name: "Ada Lovelace" })).toEqual({
      first: "ada",
      surname: "lovelace",
    });
    expect(nameParts({ name: "Ada King Lovelace" })).toEqual({
      first: "ada",
      surname: "kinglovelace",
    });
    expect(nameParts({ name: "Lovelace, Ada" })).toEqual({ first: "ada", surname: "lovelace" });
    // A given name with no ASCII letters falls back to the name, then the e-mail.
    expect(
      nameParts({ givenName: "小明", name: "王小明", email: "xiaoming.wang@example.com" }),
    ).toEqual({ first: "xiaoming", surname: "wang" });
    expect(nameParts({ email: "grace_hopper+work@example.com" })).toEqual({
      first: "grace",
      surname: "hopper",
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
    expect(() => reservedUsernames({ RESERVED_USERNAMES: "1st" })).toThrow("RESERVED_USERNAMES");
    expect([
      ...config({
        DATABASE_URL: "postgresql://subspace@127.0.0.1/subspace",
        WORKOS_CLIENT_ID: "client_example",
        RESERVED_USERNAMES: "root",
      }).reservedUsernames,
    ]).toEqual(["root"]);
  });
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

  test("a person registered as Emmy Noether is emmy, and the next Emmy Noether is emmyn", async () => {
    const emmy = await signIn({ given_name: "Emmy", family_name: "Noether", name: "Emmy Noether" });
    expect(emmy.username).toBe("emmy");
    expect(await stored(emmy.userId)).toBe("emmy");
    const second = await signIn({ given_name: "Emmy", family_name: "Noether" });
    expect(second.username).toBe("emmyn");
    const third = await signIn({ given_name: "EMMY", family_name: "Nobel" });
    expect(third.username).toBe("emmyno");
    // The username never changes by itself, whatever later tokens say.
    const again = await api.call(
      "/api/v1/me",
      await tenant.sign(crypto.randomUUID(), { given_name: "Other" }),
    );
    expect(again.status).toBe(200);
    expect(await stored(emmy.userId)).toBe("emmy");
  });

  test("concurrent registrations of people with the same first name get different usernames", async () => {
    const surnames = ["Lovelace", "Byron", "King", "Hopper", "Turing", "Noether"];
    // Hold the organisation row while every registration starts, then release them together, so
    // they race for the same name rather than arriving one after another. Without the
    // registration's own lock on that row, they all choose ada and all but one fail.
    await signIn({ name: "Organisation Seed" });
    await database.client.query("BEGIN");
    const held = await database.client.query(
      "SELECT id FROM organisations WHERE provider = 'entra' AND external_id = $1 FOR UPDATE",
      [tenant.tenant],
    );
    expect(held.rowCount).toBe(1);
    const pending = Promise.all(
      surnames.map((surname) => signIn({ given_name: "Ada", family_name: surname })),
    );
    await Bun.sleep(300);
    await database.client.query("COMMIT");
    const people = await pending;
    const names = people.map((person) => person.username!);
    expect(new Set(names).size).toBe(surnames.length);
    expect(names.filter((name) => name === "ada")).toHaveLength(1);
    // Every other one is ada followed by the start of their own surname.
    names.forEach((name, index) => {
      if (name !== "ada")
        expect(`ada${surnames[index]!.toLowerCase()}`.startsWith(name)).toBe(true);
    });
  });

  test("a person never takes an agent's IRC nick, ignoring case", async () => {
    const owner = await signIn({ name: "Owner Person" });
    const [{ organisation_id: org }] = (
      await database.client.query("SELECT organisation_id FROM users WHERE id = $1", [owner.userId])
    ).rows;
    await database.client.query(
      `INSERT INTO users (organisation_id, subject, display_name, agent_rank, agent_fleet, agent_model, registered_by_id, agent_irc_nick)
       VALUES ($1, $2, 'Grace-agent', 'crew', 'alpha', 'example-model', $3, 'Grace')`,
      [org, crypto.randomUUID(), owner.userId],
    );
    const grace = await signIn({ given_name: "Grace", family_name: "Hopper" });
    expect(grace.username).toBe("graceh");
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
    expect((await signIn({ given_name: "Zoë", family_name: "Ångström" })).username).toBe("zoe");
    expect((await signIn({ name: "Łukasz Żółw" })).username).toBe("lukasz");
    expect((await signIn({ name: "Brontë, Émilie" })).username).toBe("emilie");
    expect((await signIn({ name: "王小明", email: "xiaoming.wang@example.com" })).username).toBe(
      "xiaoming",
    );
    expect((await signIn({ preferred_username: "hedy.lamarr@example.com" })).username).toBe("hedy");
    const nameless = await signIn({ name: "王小明" });
    expect(nameless.username).toBe("user");
    expect((await signIn({})).username).toBe("user2");
  });

  test("people and agents report their usernames where the API lists people", async () => {
    const admin = await tenant.sign(crypto.randomUUID(), {
      given_name: "Annie",
      family_name: "Easley",
      roles: [adminRole],
    });
    const me = await (await api.call("/api/v1/me", admin)).json();
    expect(me.username).toBe("annie");
    const users = await (await api.call("/api/v1/users?limit=200", admin)).json();
    expect(users.items).toContainEqual(
      expect.objectContaining({ id: me.userId, username: "annie", kind: "human" }),
    );
    expect(users.items).toContainEqual(
      expect.objectContaining({ username: "Grace", kind: "agent" }),
    );
  });

  test("an agent's username follows its IRC nick, and a nick cannot take a person's username", async () => {
    const admin = await tenant.sign(crypto.randomUUID(), {
      given_name: "Mary",
      family_name: "Jackson",
      roles: [adminRole],
    });
    expect((await (await api.call("/api/v1/me", admin)).json()).username).toBe("mary");
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
    const taken = await register("MARY");
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

  test("the database refuses a second username that differs only in case", async () => {
    const person = await signIn({ given_name: "Katherine" });
    const [{ organisation_id: org }] = (
      await database.client.query("SELECT organisation_id FROM users WHERE id = $1", [
        person.userId,
      ])
    ).rows;
    await expect(
      database.client.query(
        "INSERT INTO users (organisation_id, subject, display_name, username) VALUES ($1, $2, 'Copy', 'KATHERINE')",
        [org, crypto.randomUUID()],
      ),
    ).rejects.toThrow("users_username");
  });

  test("a person without a username is given one on their next request", async () => {
    const oid = crypto.randomUUID();
    const person = await signIn({ given_name: "Dorothy", family_name: "Vaughan" }, oid);
    await database.client.query("UPDATE users SET username = NULL WHERE id = $1", [person.userId]);
    const responses = await Promise.all(
      Array.from({ length: 4 }, async () =>
        (await api.call("/api/v1/me", await tenant.sign(oid))).json(),
      ),
    );
    expect(new Set(responses.map((body) => body.username))).toEqual(new Set(["dorothy"]));
    expect(await stored(person.userId)).toBe("dorothy");
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
          new Set(["rosalind"]),
        ).pipe(Effect.provide(databaseLayer(database.url, 2))),
      );
    const saved = await provision("Rosalind Franklin");
    expect(saved.username).toBe("rosalindf");
    expect((await provision("Someone Else")).username).toBe("rosalindf");
    expect((await provision("Someone Else", false)).username).toBe("rosalindf");
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
      expect((await signIn("ada@example.com", "Ada Lovelace")).username).toBe("ada");
      expect((await signIn("root@example.com")).username).toBe("root2");
      // A second sign-in keeps the member and their username.
      expect((await signIn("dev@example.com", "Someone Else")).username).toBe("dev");
    } finally {
      await database.drop();
    }
  });
});

describe.skipIf(!testDatabase)("the username backfill", () => {
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
      // Inserted newest first, so only the backfill's order decides who is ada.
      const byron = await person(org, "Ada Byron", null, "2026-03-01T00:00:00Z");
      const lovelace = await person(org, "Ada Lovelace", null, "2026-01-01T00:00:00Z");
      const otherAda = await person(other, "Ada Clone", null, "2026-04-01T00:00:00Z");
      const alan = await person(org, "Alan Turing", null, "2026-01-02T00:00:00Z");
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
        "Alan",
        null,
        "2025-12-01T00:00:00Z",
        ", agent_rank, agent_fleet, agent_model, registered_by_id, agent_irc_nick",
        ["crew", "alpha", "example-model", lovelace, "Alan"],
      );
      const nickless = await person(
        org,
        "Nickless",
        null,
        "2025-12-02T00:00:00Z",
        ", agent_rank, agent_fleet, agent_model, registered_by_id",
        ["crew", "alpha", "example-model", lovelace],
      );

      const migrate = () => {
        const run = Bun.spawnSync([process.execPath, "scripts/migrate.ts"], {
          cwd: fileURLToPath(new URL("..", import.meta.url)),
          env: { ...process.env, DATABASE_URL: database.url, RESERVED_USERNAMES: "root" },
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
        [lovelace]: "ada",
        [byron]: "adab",
        [otherAda]: "ada",
        [alan]: "alant",
        [root]: "roota",
        [nameless]: "xiaoming",
        [former]: "former",
        [agent]: "Alan",
        [nickless]: null,
      });
      migrate();
      expect(await names()).toEqual(result);
    } finally {
      await database.drop();
    }
  });
});
