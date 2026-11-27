import { eq, sql } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { AgentName, checkAgentNames, IrcNick } from "./agents";
import { Database } from "./db";
import {
  enrolledAgent,
  insertEnrolmentToken,
  maxEnrolmentChecks,
  registerEnrolledAgent,
} from "./enrolment";
import { lockInitiativeFleet, lockProjectFleet } from "./fleet-ownership";
import {
  createFleet,
  CreateFleet,
  createShip,
  findFleet,
  IrcChannel,
  listShips,
  renameShip,
  ShipName,
  updateFleet,
} from "./fleets";
import { repositoryProjects, SetRepos } from "./github";
import { type Member } from "./identity";
import { assignFleet, createInitiative, getInitiative, listInitiatives } from "./initiatives";
import { BootstrapRefused, ensureOrganisation, ensureOwner } from "./pairing-owner";
import { auditLogs, users } from "./schema";
import { ReservedUsernames } from "./usernames";
import { Email, Line, Name, Uuid } from "./validation";

const officer = { name: AgentName, ircNick: IrcNick, model: Line(100) };
const fleetSpec = Schema.Struct({
  ...CreateFleet.fields,
  starbaseChannel: IrcChannel,
  commandChannel: IrcChannel,
});
export const FleetProvisionSpec = Schema.Struct({
  organisation: Schema.String.check(Schema.isPattern(/^[a-z0-9][a-z0-9.-]{0,62}$/u)),
  owner: Schema.optional(Schema.Struct({ name: Name, email: Schema.optional(Email) })),
  fleet: fleetSpec,
  rearAdmiral: Schema.Struct(officer),
  captains: Schema.Array(Schema.Struct({ ...officer, ship: ShipName, channel: IrcChannel })).check(
    Schema.isMaxLength(maxEnrolmentChecks - 1),
  ),
  initiative: Schema.optional(Schema.Struct({ id: Uuid, adopt: Schema.optional(Schema.Boolean) })),
  repositories: Schema.optional(SetRepos.fields.repositories),
});

type Spec = typeof FleetProvisionSpec.Type;
type Db = (typeof Database)["Service"];
type Fleet = { organisationId: string; id: string; slug: string };
const grantLifetime = 600;
const lower = (value: string) => value.toLowerCase();

function validateRoster(spec: Spec) {
  const officers = [spec.rearAdmiral, ...spec.captains];
  const channels = [
    spec.fleet.starbaseChannel,
    spec.fleet.commandChannel,
    ...spec.captains.map((c) => c.channel),
  ];
  for (const [label, values] of [
    ["Officer names", officers.map((o) => o.name)],
    ["IRC nicks", officers.map((o) => o.ircNick)],
    ["Ship names", spec.captains.map((o) => o.ship)],
    ["IRC channels", channels],
  ] as const) {
    if (new Set(values.map(lower)).size !== values.length)
      return Effect.fail(new BootstrapRefused(`${label} must differ, ignoring case.`));
  }
  return Effect.void;
}

type AuditFields = Pick<
  typeof auditLogs.$inferInsert,
  "action" | "entityType" | "entityId" | "summary" | "changes"
>;
const audit = (tx: Db, organisationId: string, fields: AuditFields) =>
  tx.insert(auditLogs).values({
    ...fields,
    organisationId,
    actorType: "integration",
    actorId: null,
    actorDisplayName: "Fleet provisioning",
    source: "cli",
  });

function reconcileFleet(tx: Db, owner: Member, spec: Spec["fleet"]) {
  return Effect.gen(function* () {
    const { organisationId } = owner;
    const [old] = yield* findFleet(tx, organisationId, spec.slug, true);
    const saved = yield* old
      ? updateFleet(owner, old.id, { ...spec, version: old.version })
      : createFleet(owner, spec);
    if (!old || saved.version !== old.version)
      yield* audit(tx, organisationId, {
        action: old ? "fleetUpdated" : "fleetCreated",
        entityType: "Fleet",
        entityId: saved.id,
        summary: "Reconciled the fleet on the API host.",
        changes: { slug: saved.slug, version: saved.version },
      });
    return { id: saved.id, slug: saved.slug, organisationId };
  });
}

function reconcileRoster(tx: Db, owner: Member, fleet: Fleet, spec: Spec) {
  return Effect.gen(function* () {
    const existingShips = new Map(
      (yield* listShips(owner, fleet.id)).map((ship) => [lower(ship.name), ship]),
    );
    const roster = [
      { ...spec.rearAdmiral, rank: "rearAdmiral" as const, ship: undefined, channel: undefined },
      ...spec.captains.map((captain) => ({ ...captain, rank: "captain" as const })),
    ];
    const officers = [];
    for (const officer of roster) {
      const existing = yield* enrolledAgent(tx, fleet, officer);
      const oldShip = officer.ship ? existingShips.get(lower(officer.ship)) : undefined;
      const ship = !officer.ship
        ? undefined
        : oldShip
          ? yield* renameShip(owner, oldShip.id, {
              name: officer.ship,
              channel: officer.channel,
              version: oldShip.version,
            })
          : yield* createShip(owner, fleet.id, officer.ship, officer.channel);
      if (ship && (!oldShip || oldShip.version !== ship.version))
        yield* audit(tx, fleet.organisationId, {
          action: oldShip ? "shipUpdated" : "shipCreated",
          entityType: "Ship",
          entityId: ship.id,
          summary: "Reconciled a ship on the API host.",
          changes: { fleetId: fleet.id, version: ship.version },
        });

      let id: string;
      if (!existing) {
        id = yield* registerEnrolledAgent(tx, fleet, ship, officer, owner.id);
      } else {
        id = existing.id;
        const changes = {
          displayName: officer.name,
          agentIrcNick: officer.ircNick,
          username: officer.ircNick,
          agentModel: officer.model,
          agentShip: ship?.name ?? null,
          agentShipId: ship?.id ?? null,
        };
        if (
          Object.entries(changes).every(
            ([key, value]) => existing[key as keyof typeof changes] === value,
          )
        ) {
          officers.push({ name: officer.name, id });
          continue;
        }
        yield* checkAgentNames(
          tx,
          fleet.organisationId,
          { subject: existing.subject, ...changes },
          existing,
        );
        yield* tx
          .update(users)
          .set({ ...changes, version: existing.version + 1, updatedAt: sql`now()` })
          .where(eq(users.id, id));
      }
      yield* audit(tx, fleet.organisationId, {
        action: existing ? "agentUpdated" : "agentRegistered",
        entityType: "Agent",
        entityId: id,
        summary: "Reconciled an officer on the API host.",
        changes: { fleetId: fleet.id, rank: officer.rank, shipId: ship?.id ?? null },
      });
      officers.push({ name: officer.name, id });
    }
    return officers;
  });
}

function findInitiative(tx: Db, owner: Member, fleet: Fleet, name: string) {
  return Effect.gen(function* () {
    let cursor: string | undefined;
    const assigned: string[] = [],
      unassigned: string[] = [];
    do {
      const page = yield* listInitiatives(owner, { search: name, limit: 200, cursor });
      for (const row of page.items) {
        if (row.name !== name) continue;
        const effectiveFleet = yield* lockInitiativeFleet(tx, fleet.organisationId, row.id);
        if (effectiveFleet === fleet.id) assigned.push(row.id);
        if (effectiveFleet === null) unassigned.push(row.id);
      }
      cursor = page.nextCursor ?? undefined;
    } while (cursor);
    const candidates = assigned.length ? assigned : unassigned;
    if (candidates.length > 1)
      return yield* Effect.fail(
        new BootstrapRefused("Several initiatives match. Supply the intended initiative ID."),
      );
    return candidates[0];
  });
}

function reconcileInitiative(tx: Db, owner: Member, fleet: Fleet, spec: Spec) {
  return Effect.gen(function* () {
    const id = spec.initiative?.id ?? (yield* findInitiative(tx, owner, fleet, spec.fleet.name));
    const row = id
      ? yield* getInitiative(owner, id)
      : yield* createInitiative(owner, {
          name: spec.fleet.name,
          ownerId: owner.id,
          status: "active",
          description: `Work for the ${spec.fleet.name} fleet. Talk to the rear admiral in ${spec.fleet.starbaseChannel}.`,
          channelConfiguration: { memberUserIds: [owner.id] },
        });
    if (row.archived || row.ancestorArchived)
      return yield* Effect.fail(
        new BootstrapRefused("Unarchive the initiative branch before provisioning."),
      );
    const previousFleetId = yield* lockInitiativeFleet(tx, fleet.organisationId, row.id);
    if (previousFleetId === fleet.id) return row.id;
    if (previousFleetId !== null && !spec.initiative?.adopt)
      return yield* Effect.fail(
        new BootstrapRefused(
          "This initiative belongs to another fleet. Explicitly adopt it to move it.",
        ),
      );
    yield* assignFleet(owner, row.id, fleet.id);
    yield* audit(tx, fleet.organisationId, {
      action: id ? "initiativeFleetAssigned" : "fleetInitiativeCreated",
      entityType: "Initiative",
      entityId: row.id,
      summary: id
        ? "Assigned an initiative on the API host."
        : "Created the fleet initiative on the API host.",
      changes: { fleetId: fleet.id, previousFleetId },
    });
    return row.id;
  });
}

function checkProjects(tx: Db, owner: Member, repositories: readonly string[]) {
  return Effect.gen(function* () {
    const checks = [];
    for (const repository of new Set(repositories.map(lower))) {
      let cursor: string | undefined;
      do {
        const page = yield* repositoryProjects(owner, { repository, cursor, limit: 200 });
        for (const project of page.items) {
          const fleetId = yield* lockProjectFleet(tx, owner.organisationId, project.id);
          checks.push({ repository, id: project.id, key: project.key, fleetId });
        }
        cursor = page.nextCursor ?? undefined;
      } while (cursor);
    }
    return checks;
  });
}

/** One transaction: a failed check must not leave a partially provisioned fleet or a usable grant. */
export const provisionFleet = (spec: Spec, reserved: ReadonlySet<string>) =>
  Effect.gen(function* () {
    yield* validateRoster(spec);
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        const organisation = yield* ensureOrganisation(tx, spec.organisation);
        const { owner } = yield* ensureOwner(tx, organisation.id, {
          displayName: spec.owner?.name,
          email: spec.owner?.email,
          reserved,
        });
        const fleet = yield* reconcileFleet(tx, owner, spec.fleet);
        const officers = yield* reconcileRoster(tx, owner, fleet, spec);
        const initiativeId = yield* reconcileInitiative(tx, owner, fleet, spec);
        const projects = yield* checkProjects(tx, owner, spec.repositories ?? []);
        const grant = yield* insertEnrolmentToken(tx, {
          organisationId: organisation.id,
          fleetId: fleet.id,
          createdById: owner.id,
          lifetime: grantLifetime,
          maxUses: officers.length,
        });
        yield* audit(tx, organisation.id, {
          action: "enrolmentTokenCreated",
          entityType: "EnrolmentToken",
          entityId: grant.id,
          summary: "Issued a fleet enrolment grant on the API host.",
          changes: { fleetId: fleet.id, expiresAt: grant.expiresAt, maxUses: grant.maxUses },
        });
        return {
          organisationId: organisation.id,
          ownerId: owner.id,
          fleetId: fleet.id,
          initiativeId,
          officers,
          projects,
          grant,
        };
      }).pipe(
        Effect.provideService(Database, tx),
        Effect.provideService(ReservedUsernames, reserved),
      ),
    );
  });
