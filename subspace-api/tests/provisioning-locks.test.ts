import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { eq } from "drizzle-orm";
import { Effect } from "effect";
import pg from "pg";
import { Database, databaseLayer } from "../src/db";
import { createFleet, createShip } from "../src/fleets";
import { FleetProvisionSpec, provisionFleet } from "../src/provisioning";
import { users } from "../src/schema";
import { isolatedDatabase, lockWaiters, testDatabase } from "./entra-stack";

/**
 * Fleet provisioning runs in one transaction that also writes initiatives, so it takes its
 * organisation's row FOR UPDATE first; ordinary fleet and ship creation take the same row before
 * they insert. Each interleaving here ends in commits or a normal 409, never a deadlock (40P01).
 */

const reserved = new Set<string>();

function spec(organisation: string, slug = "locks", ships = ["Vega"]) {
  return {
    organisation,
    owner: { name: "Aubrey Example", email: "aubrey@example.com" },
    fleet: {
      slug,
      name: `Fleet ${slug}`,
      ircHost: "irc.example.com",
      ircPort: 6697,
      ircTls: true,
      starbaseChannel: `#${slug}-starbase`,
      commandChannel: `#${slug}-command`,
    },
    rearAdmiral: { name: `${slug}-atlas`, ircNick: `${slug}-atlas`, model: "example-model" },
    captains: ships.map((ship) => ({
      name: `${slug}-${ship.toLowerCase()}`,
      ircNick: `${slug}-${ship.toLowerCase()}`,
      model: "example-model",
      ship,
      channel: `#${slug}-${ship.toLowerCase()}`,
    })),
  } satisfies typeof FleetProvisionSpec.Type;
}

/** The SQLSTATE of a database failure, however deeply Drizzle and Effect wrap it. */
function sqlState(error: unknown, seen = new Set<unknown>()): string | undefined {
  if (typeof error !== "object" || error === null || seen.has(error)) return undefined;
  seen.add(error);
  const { code } = error as { code?: unknown };
  if (typeof code === "string" && /^[0-9A-Z]{5}$/u.test(code)) return code;
  for (const key of ["cause", "error", "reasons", "errors", "failures"]) {
    const inner = (error as Record<string, unknown>)[key];
    for (const item of Array.isArray(inner) ? inner : [inner]) {
      const found = sqlState(item, seen);
      if (found) return found;
    }
  }
  return undefined;
}

describe.skipIf(!testDatabase)(
  "fleet provisioning against concurrent fleet and ship writes",
  () => {
    let database: Awaited<ReturnType<typeof isolatedDatabase>>;

    beforeAll(async () => {
      database = await isolatedDatabase("provision_locks");
    });
    afterAll(async () => {
      await database?.drop();
    });

    const run = <A, E>(effect: Effect.Effect<A, E, Database>) =>
      Effect.runPromise(effect.pipe(Effect.provide(databaseLayer(database.url))));
    const provision = (input: typeof FleetProvisionSpec.Type) =>
      run(provisionFleet(input, reserved));
    const member = async (id: string) =>
      (
        await run(Effect.flatMap(Database, (db) => db.select().from(users).where(eq(users.id, id))))
      )[0]!;

    /**
     * Holds `rowLock` on another connection, starts `provisioning`, which waits on that row after it
     * has locked its organisation (and, on a rerun, its fleet), then starts `ordinary`, and releases
     * the row once `ordinary` waits too or has finished. Returns both outcomes.
     */
    async function interleave(
      rowLock: [string, unknown[]],
      provisioning: () => Promise<unknown>,
      ordinary: () => Promise<unknown>,
    ) {
      const holder = new pg.Client({ connectionString: database.url });
      // Another connection watches for waiters: inside the holder's transaction, pg_stat_activity
      // would stay the snapshot its first read took.
      const observer = new pg.Client({ connectionString: database.url });
      await holder.connect();
      await observer.connect();
      let first: Promise<unknown> | undefined;
      let second: Promise<unknown> | undefined;
      try {
        await holder.query("BEGIN");
        await holder.query(...rowLock);
        first = provisioning();
        first.catch(() => {});
        await lockWaiters(observer, 1);
        second = ordinary();
        const settled = second.then(
          () => {},
          () => {},
        );
        // Waiting on the lock or already done; either way the row can go.
        await Promise.race([settled, lockWaiters(observer, 2).catch(() => {})]);
      } finally {
        await holder.query("COMMIT").catch(() => {});
        await holder.end();
        await observer.end();
      }
      return Promise.allSettled([first!, second!]);
    }
    const noDeadlock = (outcomes: PromiseSettledResult<unknown>[]) => {
      for (const outcome of outcomes)
        if (outcome.status === "rejected") expect(sqlState(outcome.reason)).not.toBe("40P01");
    };
    const shipLock = (fleetId: string, name: string): [string, unknown[]] => [
      "SELECT id FROM ships WHERE fleet_id = $1 AND name = $2 FOR UPDATE",
      [fleetId, name],
    ];

    test("a rerun that creates an initiative and an ordinary new ship both commit", async () => {
      const input = spec("locks-initiative", "initiative");
      const first = await provision(input);
      const owner = await member(first.ownerId);
      // With the fleet's initiative archived, the rerun creates a new one.
      await database.client.query("UPDATE initiatives SET archived = true WHERE id = $1", [
        first.initiativeId,
      ]);
      const [rerun, ship] = await interleave(
        shipLock(first.fleetId, "Vega"),
        () => provision(input),
        () => run(createShip(owner, first.fleetId, "Other")),
      );
      noDeadlock([rerun!, ship!]);
      expect(rerun!.status).toBe("fulfilled");
      expect(ship!.status).toBe("fulfilled");
      expect(
        (rerun as PromiseFulfilledResult<{ initiativeId: string }>).value.initiativeId,
      ).not.toBe(first.initiativeId);
    });

    test("a rerun and an ordinary request adding the same ship end in one ship and a 409", async () => {
      const input = spec("locks-ship", "ship");
      const first = await provision(input);
      const owner = await member(first.ownerId);
      const [rerun, ship] = await interleave(
        shipLock(first.fleetId, "Vega"),
        () => provision(spec("locks-ship", "ship", ["Vega", "Newship"])),
        () => run(createShip(owner, first.fleetId, "Newship")),
      );
      noDeadlock([rerun!, ship!]);
      expect(rerun!.status).toBe("fulfilled");
      expect(ship!.status).toBe("rejected");
      expect((ship as PromiseRejectedResult).reason).toMatchObject({ status: 409 });
      const { rows } = await database.client.query(
        "SELECT count(*)::int AS n FROM ships WHERE fleet_id = $1 AND name = 'Newship'",
        [first.fleetId],
      );
      expect(rows[0].n).toBe(1);
    });

    test("provisioning a second fleet and an ordinary fleet with its slug end in one fleet and a 409", async () => {
      const first = await provision(spec("locks-fleet", "fleet"));
      const owner = await member(first.ownerId);
      const second = spec("locks-fleet", "second");
      const [provisioned, fleet] = await interleave(
        ["SELECT id FROM users WHERE id = $1 FOR UPDATE", [first.ownerId]],
        () => provision(second),
        () => run(createFleet(owner, { ...second.fleet, name: "Ordinary second" })),
      );
      noDeadlock([provisioned!, fleet!]);
      expect(provisioned!.status).toBe("fulfilled");
      // Ordinary creation either waits and is refused, or commits first and provisioning adopts it.
      if (fleet!.status === "rejected")
        expect((fleet as PromiseRejectedResult).reason).toMatchObject({ status: 409 });
      const { rows } = await database.client.query(
        "SELECT count(*)::int AS n FROM fleets WHERE organisation_id = $1 AND slug = 'second'",
        [first.organisationId],
      );
      expect(rows[0].n).toBe(1);
    });
  },
);
