import { and, eq, isNull } from "drizzle-orm";
import { Context, Effect, Schema } from "effect";
import { type PrincipalIdentity } from "./auth";
import { type Config } from "./config";
import { Database } from "./db";
import { fleets, initiativeMemberships, initiatives, organisations, ships, users } from "./schema";

export const AgentRank = Schema.Literals(["rearAdmiral", "captain", "crew"]);
export const memberKind = (member: { agentRank: string | null }) =>
  member.agentRank ? ("agent" as const) : ("human" as const);
export type Member = typeof users.$inferSelect & {
  source?: "rest" | "mcp";
  relayedCrew?: { name: string; model: string } | null;
};
export class CurrentMember extends Context.Service<
  CurrentMember,
  Member & { tokenExpiresAt: number }
>()("subspace/CurrentMember") {}

export const findMember = (identity: PrincipalIdentity) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const [row] = yield* db
      .select({ member: users, fleet: fleets.slug, ship: ships.name })
      .from(users)
      .leftJoin(fleets, eq(fleets.id, users.agentFleetId))
      .leftJoin(ships, eq(ships.id, users.agentShipId))
      .innerJoin(organisations, eq(users.organisationId, organisations.id))
      .where(
        and(
          eq(users.subject, identity.subject),
          eq(organisations.provider, identity.provider),
          eq(organisations.externalId, identity.externalOrganisation),
        ),
      );
    return row
      ? {
          ...row.member,
          agentFleet: row.fleet ?? row.member.agentFleet,
          agentShip: row.ship ?? row.member.agentShip,
        }
      : undefined;
  });

interface Profile {
  email: string | null;
  displayName: string;
}

export const provisionMember = (
  identity: PrincipalIdentity,
  profile: Profile,
  role: Member["role"],
  active = true,
) =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        yield* tx
          .insert(organisations)
          .values({ provider: identity.provider, externalId: identity.externalOrganisation })
          .onConflictDoNothing();
        const org = (yield* tx
          .select()
          .from(organisations)
          .where(
            and(
              eq(organisations.provider, identity.provider),
              eq(organisations.externalId, identity.externalOrganisation),
            ),
          ))[0]!;
        const [member] = yield* tx
          .insert(users)
          .values({ organisationId: org.id, subject: identity.subject, ...profile, role, active })
          .onConflictDoUpdate({
            target: [users.organisationId, users.subject],
            set: active ? { ...profile, role, active } : { active },
            setWhere: isNull(users.agentRank),
          })
          .returning();
        if (!member)
          return yield* Effect.fail(new Error("This subject is registered as an agent."));
        return member;
      }),
    );
  });

export const localMember = (
  email: string,
  displayName: string,
  externalId: string,
  dev: NonNullable<Config["dev"]>,
) =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        const [newOrg] = yield* tx
          .insert(organisations)
          .values({ provider: "dev", externalId })
          .onConflictDoNothing()
          .returning();
        const org =
          newOrg ??
          (yield* tx
            .select()
            .from(organisations)
            .where(
              and(eq(organisations.provider, "dev"), eq(organisations.externalId, externalId)),
            ))[0]!;
        const [newMember] = yield* tx
          .insert(users)
          .values({
            organisationId: org.id,
            subject: email,
            email,
            displayName,
            role: newOrg ? "admin" : "member",
          })
          .onConflictDoNothing()
          .returning();
        const member =
          newMember ??
          (yield* tx
            .select()
            .from(users)
            .where(and(eq(users.organisationId, org.id), eq(users.subject, email))))[0]!;

        if (newOrg) {
          const [fleet] = yield* tx
            .insert(fleets)
            .values({
              organisationId: org.id,
              slug: "local",
              name: "Local fleet",
              ircHost: dev.ircHost,
              ircPort: dev.ircPort,
              ircTls: false,
            })
            .returning();
          const [initiative] = yield* tx
            .insert(initiatives)
            .values({
              organisationId: org.id,
              name: "Local workspace",
              channels: dev.channels,
              ownerId: member.id,
              fleetId: fleet!.id,
              status: "active",
            })
            .returning();
          yield* tx.insert(initiativeMemberships).values({
            organisationId: org.id,
            initiativeId: initiative!.id,
            userId: member.id,
          });
        }

        return member;
      }),
    );
  });

export const publicProfile = (member: Member) => ({
  userId: member.id,
  kind: memberKind(member),
  displayName: member.displayName,
  email: member.email,
  agentName: member.agentRank ? member.displayName : null,
  agentRank: member.agentRank,
  fleet: member.agentFleet,
  ship: member.agentShip,
  fleetId: member.agentFleetId,
  shipId: member.agentShipId,
  isAdmin: member.role === "admin" && !member.agentRank,
});
