import { and, eq } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { HttpApiEndpoint, HttpApiGroup, HttpApiSchema } from "effect/http-api";
import { recordAudit } from "./audit";
import { Database } from "./db";
import { CurrentMember, type Member } from "./identity";
import { EditStakeholder, NewStakeholder, Stakeholder } from "./initiative-contract";
import { getInitiative, lockInitiativeWrites, stakeholderDto } from "./initiatives";
import { BearerAuth, handleDb, ValidationErrors, versionedApi } from "./middleware";
import { endpointErrors, problem } from "./problems";
import { initiativeStakeholders } from "./schema";
import { Uuid } from "./validation";

type Change =
  | { create: typeof NewStakeholder.Type }
  | { id: string; edit: typeof EditStakeholder.Type }
  | { id: string; remove: true };

const scope = (member: Member, initiativeId: string, id: string) =>
  and(
    eq(initiativeStakeholders.organisationId, member.organisationId),
    eq(initiativeStakeholders.initiativeId, initiativeId),
    eq(initiativeStakeholders.id, id),
  );

function getStakeholder(member: Member, initiativeId: string, id: string) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const [row] = yield* db
      .select()
      .from(initiativeStakeholders)
      .where(scope(member, initiativeId, id));
    if (!row) return yield* Effect.fail(problem(404, "Stakeholder not found."));
    return stakeholderDto(row);
  });
}

function changeStakeholder(member: Member, initiativeId: string, change: Change) {
  return Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockInitiativeWrites(tx, member.organisationId);
        const initiative = yield* getInitiative(member, initiativeId).pipe(
          Effect.provideService(Database, tx),
        );
        if (member.agentRank || (member.role !== "admin" && initiative.ownerId !== member.id))
          return yield* Effect.fail(
            problem(
              403,
              "Stakeholder changes require the initiative owner or an organisation administrator.",
            ),
          );
        if (initiative.archived || initiative.ancestorArchived)
          return yield* Effect.fail(
            problem(409, "Unarchive the initiative branch before changing stakeholders."),
          );
        const stored =
          "id" in change ? initiative.stakeholders.find((row) => row.id === change.id) : undefined;
        if ("id" in change && !stored)
          return yield* Effect.fail(problem(404, "Stakeholder not found."));
        if ("remove" in change) {
          yield* tx.delete(initiativeStakeholders).where(scope(member, initiativeId, change.id));
          yield* recordAudit(tx, member, {
            action: "initiativeStakeholderDeleted",
            entityType: "InitiativeStakeholder",
            entityId: change.id,
            summary: "Removed an initiative stakeholder.",
            changes: { initiativeId },
          });
          return stakeholderDto({
            ...stored!,
            organisationId: member.organisationId,
            initiativeId,
          });
        }
        if ("edit" in change && !Object.keys(change.edit).length)
          return yield* Effect.fail(problem(400, "Supply a stakeholder field to update."));
        const values = "create" in change ? change.create : { ...stored!, ...change.edit };
        if (!values.slackHandle && !values.teamsHandle)
          return yield* Effect.fail(problem(400, "Supply at least one Slack or Teams handle."));
        const [row] =
          "create" in change
            ? yield* tx
                .insert(initiativeStakeholders)
                .values({ ...change.create, initiativeId, organisationId: member.organisationId })
                .returning()
            : yield* tx
                .update(initiativeStakeholders)
                .set(change.edit)
                .where(scope(member, initiativeId, change.id))
                .returning();
        if (!row) return yield* Effect.fail(problem(500, "The stakeholder could not be saved."));
        yield* recordAudit(tx, member, {
          action:
            "create" in change ? "initiativeStakeholderCreated" : "initiativeStakeholderUpdated",
          entityType: "InitiativeStakeholder",
          entityId: row.id,
          summary: "Changed an initiative stakeholder.",
          changes: { initiativeId },
        });
        return stakeholderDto(row);
      }),
    ),
  );
}

const collection = "/initiatives/:id/stakeholders";
const item = `${collection}/:stakeholderId`;
const params = { id: Uuid };
const itemParams = { ...params, stakeholderId: Uuid };
export const stakeholderGroup = HttpApiGroup.make("stakeholders")
  .add(
    HttpApiEndpoint.get("list", collection, {
      params,
      success: Schema.Struct({ items: Schema.Array(Stakeholder) }),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("get", item, {
      params: itemParams,
      success: Stakeholder,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("create", collection, {
      params,
      payload: NewStakeholder,
      success: Stakeholder.pipe(HttpApiSchema.status(201)),
      error: endpointErrors,
    }),
    HttpApiEndpoint.patch("edit", item, {
      params: itemParams,
      payload: EditStakeholder,
      success: Stakeholder,
      error: endpointErrors,
    }),
    HttpApiEndpoint.delete("delete", item, {
      params: itemParams,
      success: HttpApiSchema.NoContent,
      error: endpointErrors,
    }),
  )
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const stakeholderHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof stakeholderGroup>,
) =>
  handlers
    .handle("list", ({ params }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          Effect.map(getInitiative(member, params.id), ({ stakeholders }) => ({
            items: stakeholders,
          })),
        ),
      ),
    )
    .handle("get", ({ params }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          getStakeholder(member, params.id, params.stakeholderId),
        ),
      ),
    )
    .handle("create", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          changeStakeholder(member, params.id, { create: payload }),
        ),
      ),
    )
    .handle("edit", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          changeStakeholder(member, params.id, { id: params.stakeholderId, edit: payload }),
        ),
      ),
    )
    .handle("delete", ({ params }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          Effect.asVoid(
            changeStakeholder(member, params.id, { id: params.stakeholderId, remove: true }),
          ),
        ),
      ),
    );
