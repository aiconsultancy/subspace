import { Effect, Schema } from "effect";
import { AgentName, IrcNick, reconcileFleetOfficer } from "./agents";
import { agentTokenAudience, authProviders, pairingAudience, reservedUsernames } from "./config";
import { Database, databaseUrl } from "./db";
import { insertEnrolmentToken, maxEnrolmentChecks } from "./enrolment";
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
import { repositoryProjectReferences, SetRepos } from "./github";
import { type Member } from "./identity";
import { assignFleet, createInitiative, initiativePlacements } from "./initiatives";
import {
  BootstrapRefused,
  ensureOrganisation,
  ensureOwner,
  PairingOrganisationId,
} from "./pairing-owner";
import { problem } from "./problems";
import { auditLogs } from "./schema";
import { ReservedUsernames } from "./usernames";
import { Email, Line, Name, Uuid } from "./validation";

const officer = { name: AgentName, ircNick: IrcNick, model: Line(100) };
const fleetSpec = Schema.Struct({
  ...CreateFleet.fields,
  starbaseChannel: IrcChannel,
  commandChannel: IrcChannel,
});
export const FleetProvisionSpec = Schema.Struct({
  organisation: PairingOrganisationId,
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

type RosterOfficer =
  | (Spec["rearAdmiral"] & { rank: "rearAdmiral" })
  | (Spec["captains"][number] & { rank: "captain" });

function reconcileOfficerShip(
  tx: Db,
  owner: Member,
  fleet: Fleet,
  officer: RosterOfficer,
  existingShips: ReadonlyMap<string, Effect.Success<ReturnType<typeof listShips>>[number]>,
) {
  return Effect.gen(function* () {
    if (officer.rank === "rearAdmiral" || !officer.ship) return undefined;
    const old = existingShips.get(lower(officer.ship));
    const ship = yield* old
      ? renameShip(owner, old.id, {
          name: officer.ship,
          channel: officer.channel,
          version: old.version,
        })
      : createShip(owner, fleet.id, officer.ship, officer.channel);
    if (!old || old.version !== ship.version)
      yield* audit(tx, fleet.organisationId, {
        action: old ? "shipUpdated" : "shipCreated",
        entityType: "Ship",
        entityId: ship.id,
        summary: "Reconciled a ship on the API host.",
        changes: { fleetId: fleet.id, version: ship.version },
      });
    return ship;
  });
}

function reconcileRoster(tx: Db, owner: Member, fleet: Fleet, spec: Spec) {
  return Effect.gen(function* () {
    const existingShips = new Map(
      (yield* listShips(owner, fleet.id)).map((ship) => [lower(ship.name), ship]),
    );
    const roster: RosterOfficer[] = [
      { ...spec.rearAdmiral, rank: "rearAdmiral" },
      ...spec.captains.map((captain) => ({ ...captain, rank: "captain" as const })),
    ];
    const officers = [];
    for (const officer of roster) {
      const { id, shipId, outcome } = yield* reconcileFleetOfficer(
        tx,
        fleet,
        officer,
        owner.id,
        reconcileOfficerShip(tx, owner, fleet, officer, existingShips),
      );
      if (outcome !== "unchanged")
        yield* audit(tx, fleet.organisationId, {
          action: outcome === "created" ? "agentRegistered" : "agentUpdated",
          entityType: "Agent",
          entityId: id,
          summary: "Reconciled an officer on the API host.",
          changes: { fleetId: fleet.id, rank: officer.rank, shipId },
        });
      officers.push({ name: officer.name, id });
    }
    return officers;
  });
}

function selectInitiative(tx: Db, fleet: Fleet, spec: Spec) {
  return Effect.gen(function* () {
    if (spec.initiative) {
      const [row] = yield* initiativePlacements(tx, fleet.organisationId, {
        id: spec.initiative.id,
      });
      if (!row) return yield* Effect.fail(problem(404, "Initiative not found."));
      if (row.archived)
        return yield* Effect.fail(
          new BootstrapRefused("Unarchive the initiative branch before provisioning."),
        );
      return row.id;
    }
    const matches = yield* initiativePlacements(tx, fleet.organisationId, {
      name: spec.fleet.name,
    });
    const active = matches.filter((row) => !row.archived);
    const assigned = active.filter((row) => row.fleetId === fleet.id);
    const candidates = assigned.length ? assigned : active.filter((row) => row.fleetId === null);
    if (candidates.length > 1)
      return yield* Effect.fail(
        new BootstrapRefused("Several initiatives match. Supply the intended initiative ID."),
      );
    return candidates[0]?.id;
  });
}

function reconcileInitiative(tx: Db, owner: Member, fleet: Fleet, spec: Spec) {
  return Effect.gen(function* () {
    const id = yield* selectInitiative(tx, fleet, spec);
    const initiativeId =
      id ??
      (yield* createInitiative(owner, {
        name: spec.fleet.name,
        ownerId: owner.id,
        status: "active",
        description: `Work for the ${spec.fleet.name} fleet. Talk to the rear admiral in ${spec.fleet.starbaseChannel}.`,
        channelConfiguration: { memberUserIds: [owner.id] },
      })).id;
    const previousFleetId = yield* lockInitiativeFleet(tx, fleet.organisationId, initiativeId);
    if (previousFleetId === fleet.id) return initiativeId;
    if (previousFleetId !== null && !spec.initiative?.adopt)
      return yield* Effect.fail(
        new BootstrapRefused(
          "This initiative belongs to another fleet. Explicitly adopt it to move it.",
        ),
      );
    yield* assignFleet(owner, initiativeId, fleet.id);
    yield* audit(tx, fleet.organisationId, {
      action: id ? "initiativeFleetAssigned" : "fleetInitiativeCreated",
      entityType: "Initiative",
      entityId: initiativeId,
      summary: id
        ? "Assigned an initiative on the API host."
        : "Created the fleet initiative on the API host.",
      changes: { fleetId: fleet.id, previousFleetId },
    });
    return initiativeId;
  });
}

function checkProjects(tx: Db, organisationId: string, repositories: readonly string[]) {
  return Effect.gen(function* () {
    const references = yield* repositoryProjectReferences(tx, organisationId, repositories);
    const checks = [];
    for (const project of references) {
      const fleetId = yield* lockProjectFleet(tx, organisationId, project.id);
      checks.push({ ...project, fleetId });
    }
    return checks;
  });
}

/**
 * The API settings the provisioning command needs, checked as the API checks them before the
 * database is opened: pairing on, an agent token audience, a usable device token audience
 * (DEVICE_TOKEN_AUDIENCE, by default AGENT_TOKEN_AUDIENCE), reserved usernames and DATABASE_URL.
 * Throws an Error whose message names the setting, never its value, for the command to print.
 */
export function provisioningConfiguration(env: NodeJS.ProcessEnv) {
  try {
    const providers = authProviders(env);
    if (!providers.has("pairing") || !agentTokenAudience(env))
      throw new Error("AUTH_PROVIDERS must include pairing and AGENT_TOKEN_AUDIENCE must be set");
    pairingAudience(env, providers);
    return { reserved: reservedUsernames(env), url: databaseUrl(env) };
  } catch (error) {
    throw new Error(
      `The API configuration is not usable for provisioning: ${error instanceof Error ? error.message : "unknown error"}.`,
    );
  }
}

/** One transaction: a failed check must not leave a partially provisioned fleet or a usable grant. */
export const provisionFleet = (spec: Spec, reserved: ReadonlySet<string>) =>
  Effect.gen(function* () {
    yield* validateRoster(spec);
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        // The organisation's row FOR UPDATE first: initiative writes below need that mode, and taking
        // it later would upgrade a lock while this transaction holds fleet, ship and officer rows.
        const organisation = yield* ensureOrganisation(tx, spec.organisation, "update");
        const { owner, registered } = yield* ensureOwner(tx, organisation.id, {
          command: "auth:provision-fleet",
          displayName: spec.owner?.name,
          email: spec.owner?.email,
          reserved,
        });
        if (registered) yield* audit(tx, organisation.id, registered);
        const fleet = yield* reconcileFleet(tx, owner, spec.fleet);
        const officers = yield* reconcileRoster(tx, owner, fleet, spec);
        const initiativeId = yield* reconcileInitiative(tx, owner, fleet, spec);
        const projects = yield* checkProjects(tx, organisation.id, spec.repositories ?? []);
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
