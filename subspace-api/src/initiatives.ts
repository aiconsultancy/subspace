import { and, asc, eq, inArray, sql } from "drizzle-orm";
import { Effect } from "effect";
import { Database } from "./db";
import { requireReparentOwnership } from "./fleet-ownership";
import { loadFleetDtos } from "./fleets";
import { type Member } from "./identity";
import {
  type ChannelChanges,
  type CreateInitiative,
  type InitiativeQuery,
  type UpdateInitiative,
} from "./initiative-contract";
import { byKey, page } from "./paging";
import { projectDto } from "./projects";
import { problem } from "./problems";
import {
  fleets,
  initiativeMemberships,
  initiativeStakeholders,
  initiatives,
  organisations,
  projects,
  users,
} from "./schema";
import { dateTime, today } from "./validation";

type Row = typeof initiatives.$inferSelect;
type Db = (typeof Database)["Service"];

export const stakeholderDto = (row: typeof initiativeStakeholders.$inferSelect) => ({
  id: row.id,
  name: row.name,
  role: row.role,
  slackHandle: row.slackHandle,
  teamsHandle: row.teamsHandle,
});

function loadRows(db: Db, organisationId: string) {
  return Effect.gen(function* () {
    const rows = yield* db
      .select({
        initiative: initiatives,
        ownerName: users.displayName,
        ownerAvailable: users.active,
        fleet: fleets,
      })
      .from(initiatives)
      .innerJoin(
        users,
        and(eq(users.id, initiatives.ownerId), eq(users.organisationId, organisationId)),
      )
      .leftJoin(
        fleets,
        and(eq(fleets.id, initiatives.fleetId), eq(fleets.organisationId, organisationId)),
      )
      .where(eq(initiatives.organisationId, organisationId))
      .orderBy(asc(initiatives.name), asc(initiatives.id));
    const contacts = yield* db
      .select()
      .from(initiativeStakeholders)
      .where(eq(initiativeStakeholders.organisationId, organisationId))
      .orderBy(asc(initiativeStakeholders.name), asc(initiativeStakeholders.id));
    const fleetDtos = yield* loadFleetDtos(db, organisationId, [
      ...new Map(rows.flatMap(({ fleet }) => (fleet ? [[fleet.id, fleet] as const] : []))).values(),
    ]);
    const byInitiative = Map.groupBy(contacts, ({ initiativeId }) => initiativeId);
    return new Map(
      rows.map((row) => [
        row.initiative.id,
        {
          ...row,
          fleet: fleetDtos.get(row.initiative.fleetId ?? "") ?? null,
          stakeholders: (byInitiative.get(row.initiative.id) ?? []).map(stakeholderDto),
        },
      ]),
    );
  });
}

type Joined = Effect.Success<ReturnType<typeof loadRows>> extends Map<string, infer T> ? T : never;

function ancestors(row: Row, rows: ReadonlyMap<string, Joined>) {
  const result: Row[] = [];
  let id = row.parentId;
  while (id && result.length < 5) {
    const parent = rows.get(id)?.initiative;
    if (!parent) break;
    result.unshift(parent);
    id = parent.parentId;
  }
  return result;
}

function inArchivedBranch(row: Row, rows: ReadonlyMap<string, Joined>) {
  return row.archived || ancestors(row, rows).some((parent) => parent.archived);
}

function membership(row: Joined) {
  const { id, name, channels, fleetId } = row.initiative;
  return {
    id,
    name,
    channels,
    fleetId,
    stakeholders: row.stakeholders,
    fleet: row.fleet,
  };
}

function initiativeProjects(db: Db, member: Member) {
  return db
    .select({ project: projects, lead: users })
    .from(projects)
    .innerJoin(
      users,
      and(eq(projects.leadId, users.id), eq(projects.organisationId, users.organisationId)),
    )
    .where(eq(projects.organisationId, member.organisationId));
}

type ProjectRow = Effect.Success<ReturnType<typeof initiativeProjects>>[number];

function detail(
  row: Joined,
  rows: ReadonlyMap<string, Joined>,
  projects: readonly ProjectRow[],
  includeArchived = false,
) {
  const initiative = row.initiative;
  const path = ancestors(initiative, rows);
  const subtree = new Set(
    [...rows.values()]
      .filter(
        ({ initiative: candidate }) =>
          candidate.id === initiative.id ||
          ancestors(candidate, rows).some(({ id }) => id === initiative.id),
      )
      .map(({ initiative }) => initiative.id),
  );
  const unarchived = projects
    .map(({ project }) => project)
    .filter((p) => !p.archivedAt && p.initiativeId && subtree.has(p.initiativeId));
  return {
    ...membership(row),
    parentId: initiative.parentId,
    path: path.map(({ id, name, archived }) => ({ id, name, archived })),
    level: path.length + 1,
    childCount: [...rows.values()].filter(
      ({ initiative: child }) =>
        child.parentId === initiative.id && (includeArchived || !child.archived),
    ).length,
    status: initiative.status,
    ownerId: initiative.ownerId,
    ownerName: row.ownerName,
    ownerAvailable: row.ownerAvailable,
    targetDate: initiative.targetDate,
    archived: initiative.archived,
    ancestorArchived: path.some((parent) => parent.archived),
    rollUp: {
      projectCount: unarchived.length,
      completedCount: unarchived.filter((p) => p.status === "completed").length,
      openHealth: {
        offTrack: unarchived.filter(
          (p) => p.health === "offTrack" && p.status !== "completed" && p.status !== "canceled",
        ).length,
        atRisk: unarchived.filter(
          (p) => p.health === "atRisk" && p.status !== "completed" && p.status !== "canceled",
        ).length,
        onTrack: unarchived.filter(
          (p) => p.health === "onTrack" && p.status !== "completed" && p.status !== "canceled",
        ).length,
        noUpdate: unarchived.filter(
          (p) => !p.health && p.status !== "completed" && p.status !== "canceled",
        ).length,
      },
    },
    version: initiative.version,
    createdAt: dateTime(initiative.createdAt),
    updatedAt: dateTime(initiative.updatedAt),
    description: initiative.description,
    directProjectCount: unarchived.filter((p) => p.initiativeId === initiative.id).length,
    today: today(),
  };
}

export const memberInitiatives = (member: Member) =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction(
      (tx) =>
        Effect.gen(function* () {
          const memberships = yield* tx
            .select({ id: initiativeMemberships.initiativeId })
            .from(initiativeMemberships)
            .where(
              and(
                eq(initiativeMemberships.organisationId, member.organisationId),
                eq(initiativeMemberships.userId, member.id),
              ),
            );
          const ids = new Set(memberships.map(({ id }) => id));
          const rows = yield* loadRows(tx, member.organisationId);
          const items = [...rows.values()]
            .filter(
              ({ initiative }) => ids.has(initiative.id) && !inArchivedBranch(initiative, rows),
            )
            .map(membership);
          return { items };
        }),
      { isolationLevel: "repeatable read", accessMode: "read only" },
    );
  });

const statusRank = { active: 0, planned: 1, completed: 2, canceled: 3 } as const;
const levelKey = (row: Row) =>
  `${statusRank[row.status]}:${row.targetDate ?? "9999-12-31"}:${row.id}`;

export const listInitiatives = (member: Member, query: typeof InitiativeQuery.Type) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const rows = yield* loadRows(db, member.organisationId);
    const { search, ownerId, status, includeArchived, rootId, assignable, limit, cursor } = query;
    if (rootId && !rows.has(rootId))
      return yield* Effect.fail(problem(404, "Root initiative not found."));
    const searchTerm = search?.trim().toLowerCase();
    const flat = Boolean(searchTerm || ownerId || status);
    const matched = [...rows.values()].filter(({ initiative: row }) => {
      const archived = inArchivedBranch(row, rows);
      return (
        (includeArchived || !archived) &&
        (!searchTerm || row.name.toLowerCase().includes(searchTerm)) &&
        (!ownerId || row.ownerId === ownerId) &&
        (!status || row.status === status) &&
        (!rootId || ancestors(row, rows).some((parent) => parent.id === rootId)) &&
        (!assignable || (!archived && (row.status === "active" || row.status === "planned")))
      );
    });
    const key = ({ initiative }: Joined) =>
      flat
        ? levelKey(initiative)
        : [...ancestors(initiative, rows), initiative].map(levelKey).join("/");
    const results = yield* page(
      matched.sort(byKey(key)),
      {
        organisation: member.organisationId,
        searchTerm,
        ownerId,
        status,
        includeArchived,
        rootId,
        assignable,
      },
      key,
      limit,
      cursor,
    );
    const projects = yield* initiativeProjects(db, member);
    return {
      items: results.items.map((row) => detail(row, rows, projects, includeArchived)),
      nextCursor: results.nextCursor,
      totalCount: matched.length,
      maxLevel: matched.reduce(
        (max, { initiative }) => Math.max(max, ancestors(initiative, rows).length + 1),
        0,
      ),
      today: today(),
    };
  });

function readInitiative(db: Db, member: Member, id: string) {
  return Effect.gen(function* () {
    const rows = yield* loadRows(db, member.organisationId);
    const row = rows.get(id);
    if (!row) return yield* Effect.fail(problem(404, "Initiative not found."));
    return detail(row, rows, yield* initiativeProjects(db, member));
  });
}

export const getInitiative = (member: Member, id: string) =>
  Effect.flatMap(Database, (db) => readInitiative(db, member, id));

function activeUsers(db: Db, organisationId: string, ids: readonly string[]) {
  if (!ids.length) return Effect.succeed(true);
  return db
    .select({ id: users.id })
    .from(users)
    .where(
      and(
        eq(users.organisationId, organisationId),
        eq(users.active, true),
        inArray(users.id, [...ids]),
      ),
    )
    .pipe(Effect.map((found) => found.length === ids.length));
}

export function lockInitiativeWrites(db: Db, organisationId: string) {
  return db
    .select({ id: organisations.id })
    .from(organisations)
    .where(eq(organisations.id, organisationId))
    .for("update");
}

function allowedParent(rows: ReadonlyMap<string, Joined>, parentId: string | null, id?: string) {
  if (!parentId) return true;
  const parent = rows.get(parentId)?.initiative;
  if (!parent || parent.archived || !["planned", "active"].includes(parent.status)) return false;
  const parents = ancestors(parent, rows);
  if (parentId === id || parents.some((row) => row.id === id || row.archived)) return false;
  if (!id) return parents.length + 2 <= 5;
  const subtreeDepth = [...rows.values()].reduce((depth, { initiative }) => {
    const path = ancestors(initiative, rows);
    const at = path.findIndex((row) => row.id === id);
    return at === -1 ? depth : Math.max(depth, path.length - at);
  }, 0);
  return parents.length + 2 + subtreeDepth <= 5;
}

export const createInitiative = (member: Member, payload: typeof CreateInitiative.Type) =>
  Effect.gen(function* () {
    if (member.role !== "admin" || member.agentRank)
      return yield* Effect.fail(
        problem(403, "Initiative creation requires a human Subspace administrator."),
      );
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockInitiativeWrites(tx, member.organisationId);
        const { channelConfiguration, ...fields } = payload;
        const rows = yield* loadRows(tx, member.organisationId);
        if (!allowedParent(rows, fields.parentId ?? null)) {
          return yield* Effect.fail(
            problem(400, "Select an available parent within the five-level hierarchy."),
          );
        }
        const ids = [...new Set(channelConfiguration?.memberUserIds ?? [])];
        if (
          !(yield* activeUsers(tx, member.organisationId, [...new Set([fields.ownerId, ...ids])]))
        ) {
          return yield* Effect.fail(
            problem(400, "Owners and new memberships require active organisation members."),
          );
        }
        const [row] = yield* tx
          .insert(initiatives)
          .values({
            ...fields,
            organisationId: member.organisationId,
            channels: channelConfiguration?.channels ?? "",
          })
          .returning();
        if (!row) return yield* Effect.fail(problem(500, "Initiative could not be created."));
        if (ids.length)
          yield* tx.insert(initiativeMemberships).values(
            ids.map((userId) => ({
              organisationId: member.organisationId,
              initiativeId: row.id,
              userId,
            })),
          );
        return yield* readInitiative(tx, member, row.id);
      }),
    );
  });

const whereInitiative = (member: Member, id: string) =>
  and(eq(initiatives.organisationId, member.organisationId), eq(initiatives.id, id));

export const updateInitiative = (
  member: Member,
  id: string,
  payload: typeof UpdateInitiative.Type | { version: number; archived: boolean },
) =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockInitiativeWrites(tx, member.organisationId);
        const rows = yield* loadRows(tx, member.organisationId);
        const row = rows.get(id)?.initiative;
        if (!row) return yield* Effect.fail(problem(404, "Initiative not found."));
        if (member.role !== "admin" && row.ownerId !== member.id) {
          return yield* Effect.fail(
            problem(
              403,
              "Initiative changes require the initiative owner or an organisation admin.",
            ),
          );
        }
        const { version, ...changes } = payload;
        if (row.version !== version)
          return yield* Effect.fail(problem(409, "The initiative version is stale.", row.version));
        if (!Object.keys(changes).length)
          return yield* Effect.fail(problem(400, "Supply an initiative field to update."));
        if (
          ancestors(row, rows).some((parent) => parent.archived) ||
          (row.archived && !("archived" in changes))
        ) {
          return yield* Effect.fail(
            problem(409, "Unarchive the initiative branch before editing."),
          );
        }
        if ("parentId" in changes && changes.parentId !== row.parentId) {
          const parentId = changes.parentId ?? null;
          if (!allowedParent(rows, parentId, id)) {
            return yield* Effect.fail(
              problem(400, "Select an available parent within the five-level hierarchy."),
            );
          }
          yield* requireReparentOwnership(tx, member, id, parentId);
        }
        if (
          "ownerId" in changes &&
          changes.ownerId &&
          !(yield* activeUsers(tx, member.organisationId, [changes.ownerId]))
        ) {
          return yield* Effect.fail(
            problem(400, "The owner must be an active organisation member."),
          );
        }
        yield* tx
          .update(initiatives)
          .set({
            ...changes,
            version: sql`${initiatives.version} + 1`,
            updatedAt: sql`now()`,
          })
          .where(whereInitiative(member, id));
        return yield* readInitiative(tx, member, id);
      }),
    );
  });

export const assignFleet = (member: Member, id: string, fleetId: string | null) =>
  Effect.gen(function* () {
    if (member.role !== "admin")
      return yield* Effect.fail(
        problem(403, "Fleet assignment requires organisation admin access."),
      );
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockInitiativeWrites(tx, member.organisationId);
        const rows = yield* loadRows(tx, member.organisationId);
        const row = rows.get(id)?.initiative;
        if (!row) return yield* Effect.fail(problem(404, "Initiative not found."));
        if (inArchivedBranch(row, rows))
          return yield* Effect.fail(
            problem(409, "Unarchive the initiative branch before editing."),
          );
        if (fleetId) {
          const [fleet] = yield* tx
            .select({ id: fleets.id })
            .from(fleets)
            .where(and(eq(fleets.organisationId, member.organisationId), eq(fleets.id, fleetId)));
          if (!fleet) return yield* Effect.fail(problem(404, "Fleet not found."));
        }
        if (row.fleetId !== fleetId)
          yield* tx
            .update(initiatives)
            .set({
              fleetId,
              version: sql`${initiatives.version} + 1`,
              updatedAt: sql`now()`,
            })
            .where(whereInitiative(member, id));
        return yield* readInitiative(tx, member, id);
      }),
    );
  });

export const updateChannels = (member: Member, id: string, payload: typeof ChannelChanges.Type) =>
  Effect.gen(function* () {
    if (member.role !== "admin")
      return yield* Effect.fail(
        problem(403, "Channel configuration requires organisation admin access."),
      );
    const { version, channels, addMemberUserIds, removeMemberUserIds } = payload;
    if (
      channels === undefined &&
      addMemberUserIds === undefined &&
      removeMemberUserIds === undefined
    ) {
      return yield* Effect.fail(problem(400, "Supply channel text or a membership change."));
    }
    const add = new Set(addMemberUserIds);
    const remove = new Set(removeMemberUserIds);
    if ([...add].some((userId) => remove.has(userId))) {
      return yield* Effect.fail(
        problem(400, "A user cannot be added and removed in the same request."),
      );
    }
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockInitiativeWrites(tx, member.organisationId);
        const rows = yield* loadRows(tx, member.organisationId);
        const row = rows.get(id)?.initiative;
        if (!row) return yield* Effect.fail(problem(404, "Initiative not found."));
        if (row.version !== version)
          return yield* Effect.fail(problem(409, "The initiative version is stale.", row.version));
        if (inArchivedBranch(row, rows))
          return yield* Effect.fail(
            problem(409, "Unarchive the initiative branch before editing."),
          );

        const scope = and(
          eq(initiativeMemberships.organisationId, member.organisationId),
          eq(initiativeMemberships.initiativeId, id),
        );
        const existing = yield* tx
          .select({ userId: initiativeMemberships.userId })
          .from(initiativeMemberships)
          .where(scope);
        const before = new Set(existing.map(({ userId }) => userId));
        const additions = [...add].filter((userId) => !before.has(userId));
        const removals = [...remove].filter((userId) => before.has(userId));
        if (!(yield* activeUsers(tx, member.organisationId, additions))) {
          return yield* Effect.fail(
            problem(400, "New memberships require active organisation members."),
          );
        }
        if (additions.length)
          yield* tx.insert(initiativeMemberships).values(
            additions.map((userId) => ({
              organisationId: member.organisationId,
              initiativeId: id,
              userId,
            })),
          );
        if (removals.length)
          yield* tx
            .delete(initiativeMemberships)
            .where(and(scope, inArray(initiativeMemberships.userId, removals)));
        const nextChannels = channels ?? row.channels;
        const changed =
          additions.length > 0 || removals.length > 0 || nextChannels !== row.channels;
        if (changed)
          yield* tx
            .update(initiatives)
            .set({
              channels: nextChannels,
              version: sql`${initiatives.version} + 1`,
              updatedAt: sql`now()`,
            })
            .where(whereInitiative(member, id));
        return {
          id,
          channels: nextChannels,
          version: version + Number(changed),
          memberUserIds: [
            ...[...before].filter((userId) => !remove.has(userId)),
            ...additions,
          ].sort(),
        };
      }),
    );
  });

export const listInitiativeProjects = (
  member: Member,
  id: string,
  query: { scope?: "all" | "direct"; limit?: number; cursor?: string },
) =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction(
      (tx) =>
        Effect.gen(function* () {
          const rows = yield* loadRows(tx, member.organisationId);
          if (!rows.has(id)) return yield* Effect.fail(problem(404, "Initiative not found."));
          const visible = new Map<
            string,
            {
              id: string;
              name: string;
              status: Row["status"];
              archived: boolean;
              ancestorArchived: boolean;
              path: { id: string; name: string; archived: boolean }[];
            }
          >();
          for (const { initiative } of rows.values()) {
            const path = ancestors(initiative, rows);
            const root = path.findIndex((parent) => parent.id === id);
            if (initiative.id === id || (query.scope !== "direct" && root >= 0)) {
              visible.set(initiative.id, {
                id: initiative.id,
                name: initiative.name,
                status: initiative.status,
                archived: initiative.archived,
                ancestorArchived: path.some((parent) => parent.archived),
                path:
                  initiative.id === id
                    ? []
                    : path
                        .slice(root + 1)
                        .map(({ id, name, archived }) => ({ id, name, archived })),
              });
            }
          }
          const projects = (yield* initiativeProjects(tx, member)).filter(
            ({ project }) =>
              !project.archivedAt && project.initiativeId && visible.has(project.initiativeId),
          );
          const key = ({ project }: ProjectRow) => `${project.name.toLowerCase()}:${project.id}`;
          const result = yield* page(
            projects.sort(byKey(key)),
            { org: member.organisationId, initiative: id, scope: query.scope ?? "all" },
            key,
            query.limit ?? 20,
            query.cursor,
          );
          const groupIds = new Set(
            result.items.flatMap(({ project }) =>
              project.initiativeId ? [project.initiativeId] : [],
            ),
          );
          return {
            items: result.items.map(({ project, lead }) => projectDto(project, lead)),
            nextCursor: result.nextCursor,
            groups: [...visible.values()].filter((item) => groupIds.has(item.id)),
            totalCount: projects.length,
            directCount: projects.filter(({ project }) => project.initiativeId === id).length,
          };
        }),
      { isolationLevel: "repeatable read", accessMode: "read only" },
    );
  });
