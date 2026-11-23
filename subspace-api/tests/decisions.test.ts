import { afterAll, beforeEach, expect, test } from "bun:test";
import { BunHttpServer } from "@effect/platform-bun";
import { Effect, Layer } from "effect";
import { HttpRouter } from "effect/http";
import { HttpApi, HttpApiBuilder } from "effect/http-api";
import { Database } from "../src/db";
import { decisionDto, decisionGroup, decisionHandlers, withMadeBy } from "../src/decisions";
import { CurrentMember, type Member } from "../src/identity";
import { BearerAuth, validationErrors } from "../src/middleware";

type Row = Parameters<typeof decisionDto>[0];

const org = "00000000-0000-4000-8000-000000000001";
const projectId = "00000000-0000-4000-8000-000000000002";
const created = new Date("2026-11-02T09:00:00.000Z");

function member(id: string, displayName: string, fields: Partial<Member> = {}): Member {
  return {
    id,
    organisationId: org,
    subject: `${id}@example.com`,
    email: `${id}@example.com`,
    displayName,
    role: "member",
    active: true,
    agentRank: null,
    agentFleet: null,
    agentFleetId: null,
    agentShipId: null,
    agentShip: null,
    agentModel: null,
    agentIrcNick: null,
    username: null,
    registeredById: null,
    deactivatedById: null,
    deactivatedAt: null,
    version: 1,
    createdAt: created,
    updatedAt: created,
    ...fields,
  };
}

const human = member("00000000-0000-4000-8000-000000000003", "Example Lead");
const departed = member("00000000-0000-4000-8000-000000000004", "Former Member", {
  active: false,
});
const agent = member("00000000-0000-4000-8000-000000000005", "Example Captain", {
  agentRank: "captain",
  agentFleet: "Example fleet",
  agentShip: "Example ship",
  agentModel: "example-model",
  agentIrcNick: null,
  username: null,
});

function row(id: string, actor: Member, status: Row["decision"]["status"] = "proposed"): Row {
  return {
    decision: {
      id,
      organisationId: org,
      projectId,
      recordedById: actor.id,
      title: "Example decision",
      supersedesId: null,
      supersededById: null,
      decidedOn: "2026-11-01",
      status,
      decision: "Record the example",
      context: "",
      why: "",
      implications: "",
      alternatives: "",
      tags: ["design"],
      madeByType: actor.agentRank ? "agent" : "human",
      madeByUserId: actor.agentRank ? null : actor.id,
      madeByAgentName: actor.agentRank ? actor.displayName : null,
      madeByAgentRank: actor.agentRank,
      madeByAgentModel: actor.agentModel,
      evidenceKind: null,
      evidenceReference: null,
      evidenceExcerpt: null,
      acceptedAt: null,
      acceptedByUserId: null,
      rejectedAt: null,
      rejectedByUserId: null,
      version: 1,
      createdAt: created,
      updatedAt: created,
    },
    project: {
      id: projectId,
      key: "EXA",
      name: "Example project",
      archivedAt: null,
    } as Row["project"],
    actor,
    maker: actor.agentRank ? null : actor,
    cursorTime: "2026-11-02T09:00:00.0000000+00:00",
  };
}

const dto = (record: Row) => decisionDto(record, [], new Map());

test("a human decision names its maker, an agent decision leaves madeBy null", () => {
  const byHuman = dto(row("00000000-0000-4000-8000-000000000010", human));
  expect(byHuman.madeByUserId).toBe(human.id);
  expect(byHuman.madeBy).toEqual({
    userId: human.id,
    displayName: "Example Lead",
    available: true,
    kind: "human",
  });
  expect(byHuman.recordedBy).toEqual(byHuman.madeBy!);

  const byAgent = dto(row("00000000-0000-4000-8000-000000000011", agent));
  expect(byAgent.madeByUserId).toBeNull();
  expect(byAgent.madeBy).toBeNull();
  expect(byAgent.madeByAgentName).toBe("Example Captain");
  expect(byAgent.recordedBy).toEqual({
    userId: agent.id,
    displayName: "Example Captain",
    available: true,
    kind: "agent",
  });
});

test("an unavailable member is still named, marked unavailable, on both fields", () => {
  const result = dto(row("00000000-0000-4000-8000-000000000012", departed));
  expect(result.madeBy).toEqual({
    userId: departed.id,
    displayName: "Former Member",
    available: false,
    kind: "human",
  });
  expect(result.recordedBy).toEqual(result.madeBy!);
});

test("revision snapshots saved before madeBy existed gain it from their own recordedBy", () => {
  const recordedBy = {
    userId: human.id,
    displayName: "Example Lead",
    available: true,
    kind: "human",
  };
  expect(withMadeBy({ madeByUserId: human.id, recordedBy })).toEqual({
    madeByUserId: human.id,
    recordedBy,
    madeBy: recordedBy,
  });
  const agentRecord = { ...recordedBy, userId: agent.id, kind: "agent" };
  expect(withMadeBy({ madeByUserId: null, recordedBy: agentRecord })).toMatchObject({
    madeBy: null,
  });
  const current = { madeByUserId: human.id, recordedBy, madeBy: null };
  expect(withMadeBy(current)).toBe(current);
});

// Serves the real decision routes with a stub database that answers every transaction with
// `reply` and records any other access, so routing and response encoding need no PostgreSQL.
let reply: unknown;
const calls: string[] = [];
const database = new Proxy(
  {},
  {
    get: (_target, name) => {
      calls.push(String(name));
      if (name === "transaction") return () => Effect.succeed(reply);
      return () => {
        throw new Error("The stub database has no rows.");
      };
    },
  },
);
const auth = Layer.succeed(
  BearerAuth,
  BearerAuth.of({
    bearer: (next) => Effect.provideService(next, CurrentMember, { ...human, tokenExpiresAt: 0 }),
  }),
);
const api = HttpApi.make("decisions-test").add(decisionGroup);
const app = HttpApiBuilder.layer(api).pipe(
  Layer.provide(HttpApiBuilder.group(api, "decisions", decisionHandlers)),
  Layer.provide(Layer.mergeAll(auth, validationErrors)),
  HttpRouter.provideRequest(Layer.succeed(Database, database as (typeof Database)["Service"])),
  Layer.provide(BunHttpServer.layerHttpServices),
);
const { handler, dispose } = HttpRouter.toWebHandler(app, { disableLogger: true });
afterAll(() => dispose());
beforeEach(() => {
  calls.length = 0;
  reply = undefined;
});

const get = (path: string) =>
  handler(
    new Request(`http://127.0.0.1/api/v1${path}`, {
      headers: { Authorization: "Bearer example" },
    }),
  );

test("list items carry recordedBy and madeBy but not the detail's fields", async () => {
  const items = [
    dto(row("00000000-0000-4000-8000-000000000020", human, "accepted")),
    dto(row("00000000-0000-4000-8000-000000000021", agent)),
  ];
  reply = { items, nextCursor: null };
  const response = await get("/decisions");
  expect(response.status).toBe(200);
  const body = (await response.json()) as { items: Record<string, unknown>[] };
  expect(body.items[0]).toMatchObject({
    recordedBy: items[0]!.recordedBy,
    madeBy: items[0]!.madeBy,
  });
  expect(body.items[1]).toMatchObject({ recordedBy: items[1]!.recordedBy, madeBy: null });
  expect(body.items[0]).not.toHaveProperty("decision");
  expect(body.items[0]).not.toHaveProperty("provenance");
});

test("the counts route is not taken by the decision ID route, nor the reverse", async () => {
  const counts = { proposed: 1, accepted: 4, rejected: 1, superseded: 2, total: 8 };
  reply = counts;
  const response = await get("/decisions/counts");
  expect(response.status).toBe(200);
  expect(await response.json()).toEqual(counts);
  expect(calls).toEqual(["transaction"]);

  calls.length = 0;
  expect((await get("/decisions/00000000-0000-4000-8000-000000000020")).status).toBe(500);
  expect(calls).toContain("select");
  expect(calls).not.toContain("transaction");
});

test("counts validate the list's filters and ignore parameters the list would not know", async () => {
  reply = { proposed: 0, accepted: 0, rejected: 0, superseded: 0, total: 0 };
  for (const query of ["madeByType=robot", "tag=%00", "project=EX%00A", "q=%00"]) {
    const list = await get(`/decisions?${query}`);
    const counts = await get(`/decisions/counts?${query}`);
    expect([query, list.status]).toEqual([query, 400]);
    expect([query, counts.status]).toEqual([query, 400]);
    expect(await counts.json()).toEqual(await list.json());
  }
  expect(calls).toEqual([]);
  for (const query of [
    "status=accepted",
    "limit=0&cursor=example",
    "unknown=1",
    "tag=",
    "project=%20EXA",
    `q=${"x".repeat(201)}`,
  ]) {
    const counts = await get(`/decisions/counts?${query}`);
    expect([query, counts.status]).toEqual([query, 200]);
    expect(await counts.json()).toEqual(reply as object);
  }
  reply = { items: [], nextCursor: null };
  expect((await get("/decisions?unknown=1")).status).toBe(200);
});
