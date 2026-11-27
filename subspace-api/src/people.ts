import { and, eq, isNull } from "drizzle-orm";
import { Effect } from "effect";
import { Database } from "./db";
import { type CurrentMember, isAdministrator, requestRole } from "./identity";
import { problem } from "./problems";
import { agentCredentials, users } from "./schema";

/**
 * The people a signed-in request acts on: the caller as they stand under their organisation's
 * lock, and the person whose devices, invites or linked accounts it manages. Shared by the device,
 * invite and linked account routes.
 */

type Db = (typeof Database)["Service"];

/** The member a bearer route's request signed in as. */
export type Caller = CurrentMember["Service"];

/** The caller as `confirmCaller` reads them under the lock, with their provisioned role apart. */
export type ConfirmedCaller = Caller & { readonly provisionedRole: Caller["role"] };

/** The refusal of another person's devices or linked accounts to anyone but an administrator. */
export const adminOnly = (caller: Caller) =>
  !isAdministrator(caller)
    ? Effect.fail(
        problem(403, "Another member's devices and accounts are managed by an administrator."),
      )
    : Effect.void;

export const memberNotFound = () =>
  problem(404, "Member was not found. Use a person's userId from the users list.");

/** The person with this ID in the caller's organisation, if there is one. */
export const findPerson = (db: Db, caller: Caller, memberId: string) =>
  Effect.map(
    db
      .select()
      .from(users)
      .where(
        and(
          eq(users.organisationId, caller.organisationId),
          eq(users.id, memberId),
          isNull(users.agentRank),
        ),
      ),
    (rows) => rows[0],
  );

/**
 * The caller as they stand now, read in a transaction that holds their organisation's lock: still
 * active, with their current role from the source they signed in with (`requestRole`: for their
 * own Entra token, never above what that token granted, see `RoleSource`), and,
 * for a request a device signed, that device still unrevoked. A deactivation, demotion or
 * revocation that committed after the request signed in then refuses it, rather than racing it,
 * and one that waits for the lock sees what it did.
 */
export const confirmCaller = (tx: Db, caller: Caller) =>
  Effect.gen(function* () {
    const [row] = yield* tx
      .select({ active: users.active, role: users.role, tokenRole: users.tokenRole })
      .from(users)
      .where(and(eq(users.organisationId, caller.organisationId), eq(users.id, caller.id)))
      .for("share");
    if (!row?.active)
      return yield* Effect.fail(problem(403, "An active organisation membership is required."));
    if (caller.keyCredential) {
      const [device] = yield* tx
        .select({ id: agentCredentials.id })
        .from(agentCredentials)
        .where(
          and(eq(agentCredentials.id, caller.keyCredential.id), isNull(agentCredentials.revokedAt)),
        )
        .for("share");
      if (!device)
        return yield* Effect.fail(
          problem(403, "The key credential that signed this request was revoked."),
        );
    }
    // The role as it stands now, from the source the request signed in with and capped by its own
    // token's grant, never guessed, and the provisioned role apart, which an invite's issuer
    // authority records.
    return {
      ...caller,
      role: requestRole(row, caller),
      tokenRole: row.tokenRole,
      provisionedRole: row.role,
    };
  });

/**
 * The person whose devices or linked accounts a request manages: the caller, for the `/me`
 * routes, or for the `/members` routes a person of the caller's organisation, which only an
 * administrator may name.
 */
export const managedPerson = (db: Db, caller: Caller, memberId: string | null) =>
  Effect.gen(function* () {
    if (memberId === null) {
      if (caller.agentRank)
        return yield* Effect.fail(
          problem(
            403,
            "An agent has no devices or linked accounts: its key credentials are under its credentials routes.",
          ),
        );
      return caller.id;
    }
    yield* adminOnly(caller);
    const person = yield* findPerson(db, caller, memberId);
    if (!person) return yield* Effect.fail(memberNotFound());
    return person.id;
  });
