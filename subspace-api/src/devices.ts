import { and, asc, desc, eq, type SQL } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { HttpApi, HttpApiEndpoint, HttpApiGroup, OpenApi } from "effect/http-api";
import { lockOrganisation } from "./agent-credentials";
import { CredentialScope, KeyAlgorithm } from "./agent-keys";
import { type AuditFields, recordAudit } from "./audit";
import { Database } from "./db";
import { revokeDevices } from "./device-revocation";
import { CurrentMember } from "./identity";
import { BearerAuth, handleDb, ValidationErrors, versionedApi } from "./middleware";
import { databaseNow } from "./pairing-invites";
import { confirmCaller, managedPerson, type Caller } from "./people";
import { endpointErrors, problem } from "./problems";
import { agentCredentials, pairingInvites } from "./schema";
import { dateTime, dateTimeOrNull, NullableString, Timestamp, Uuid } from "./validation";

/**
 * A person's paired devices. A device is a key credential of a person (see `pairing.ts`); an
 * agent's key credentials are managed through the agent credential routes, which never show a
 * person's device, and these routes never show an agent's. The invites that pair devices are in
 * `invites.ts`.
 */

type Db = (typeof Database)["Service"];

export const Device = Schema.Struct({
  id: Uuid,
  memberId: Uuid,
  fingerprint: Schema.String,
  algorithm: KeyAlgorithm,
  scopes: Schema.Array(CredentialScope),
  label: NullableString,
  createdAt: Timestamp,
  pairingInviteId: Schema.NullOr(Uuid),
  /**
   * Who issued the invite that paired it: a member, and the device that signed the request if one
   * did. Both are null for an invite from the owner bootstrap command on the API host.
   */
  invitedByUserId: Schema.NullOr(Uuid),
  invitedByDeviceId: Schema.NullOr(Uuid),
  lastUsedAt: Schema.NullOr(Timestamp),
  revokedAt: Schema.NullOr(Timestamp),
  revokedByUserId: Schema.NullOr(Uuid),
  /** Whether this device signed the request that lists it. */
  current: Schema.Boolean,
}).annotate({ identifier: "Device" });

/** Devices with who issued the invite that paired each one. */
const deviceRows = (db: Db, where: SQL | undefined) =>
  db
    .select({
      device: agentCredentials,
      invitedByUserId: pairingInvites.createdById,
      invitedByDeviceId: pairingInvites.createdByCredentialId,
    })
    .from(agentCredentials)
    .leftJoin(pairingInvites, eq(agentCredentials.pairingInviteId, pairingInvites.id))
    .where(where);

type DeviceRow = {
  device: typeof agentCredentials.$inferSelect;
  invitedByUserId: string | null;
  invitedByDeviceId: string | null;
};

const deviceDto = ({ device, invitedByUserId, invitedByDeviceId }: DeviceRow, caller: Caller) => ({
  id: device.id,
  memberId: device.userId,
  fingerprint: device.fingerprint,
  algorithm: device.algorithm,
  scopes: device.scopes,
  label: device.label,
  createdAt: dateTime(device.createdAt),
  pairingInviteId: device.pairingInviteId,
  invitedByUserId,
  invitedByDeviceId,
  lastUsedAt: dateTimeOrNull(device.lastUsedAt),
  revokedAt: dateTimeOrNull(device.revokedAt),
  revokedByUserId: device.revokedById,
  current: caller.keyCredential?.id === device.id,
});

const listDevices = (caller: Caller, memberId: string | null) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const owner = yield* managedPerson(db, caller, memberId);
    const rows = yield* deviceRows(
      db,
      and(
        eq(agentCredentials.organisationId, caller.organisationId),
        eq(agentCredentials.userId, owner),
      ),
    ).orderBy(desc(agentCredentials.createdAt), asc(agentCredentials.id));
    return { items: rows.map((row) => deviceDto(row, caller)) };
  });

/**
 * Revokes one device of a person, and what it issued (see `revokeDevices`). Its tokens are refused
 * from its next request, and its IRC passwords from its next IRC sign-in; the person's other
 * devices keep working. Revoking a revoked device returns it unchanged, after revoking anything it
 * issued that is not yet revoked.
 */
const revokeDevice = (signedIn: Caller, memberId: string | null, credentialId: string) =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockOrganisation(tx, signedIn.organisationId);
        const caller = yield* confirmCaller(tx, signedIn);
        const owner = yield* managedPerson(tx, caller, memberId);
        const [old] = yield* tx
          .select()
          .from(agentCredentials)
          .where(
            and(
              eq(agentCredentials.organisationId, caller.organisationId),
              eq(agentCredentials.userId, owner),
              eq(agentCredentials.id, credentialId),
            ),
          )
          .for("update");
        if (!old)
          return yield* Effect.fail(
            problem(404, "Device was not found. Use an ID from the member's devices."),
          );
        const revoker = {
          organisationId: caller.organisationId,
          memberId: caller.id,
          audit: (fields: AuditFields) => recordAudit(tx, caller, fields),
        };
        yield* revokeDevices(
          tx,
          revoker,
          [old],
          () =>
            owner === caller.id ? "Revoked one of their devices." : "Revoked a member's device.",
          yield* databaseNow(tx, caller.organisationId),
        );
        const [row] = yield* deviceRows(tx, eq(agentCredentials.id, old.id));
        return deviceDto(row!, caller);
      }),
    );
  });

const devices = Schema.Struct({ items: Schema.Array(Device) });
const memberParams = { memberId: Uuid };
const cascade =
  "What the device issued is revoked with it: its pending pairing invites, and the devices paired with its invites, and in turn what they issued. A revoked device is returned unchanged, after anything it issued is revoked.";
const untyped = "Its problems are about:blank.";

/** People's devices, on the main listener only. */
export const deviceGroup = HttpApiGroup.make("devices")
  .add(
    HttpApiEndpoint.get("listMyDevices", "/me/devices", {
      success: devices,
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      `The caller's paired devices, revoked ones included, newest first; current marks the device that signed the request. ${untyped}`,
    ),
    HttpApiEndpoint.delete("revokeMyDevice", "/me/devices/:credentialId", {
      params: { credentialId: Uuid },
      success: Device,
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      `Revokes one of the caller's devices, such as a lost one. Its next API request and its next IRC sign-in are refused, even with an IRC password it already holds; the caller's other devices keep working. ${cascade} ${untyped}`,
    ),
    HttpApiEndpoint.get("listMemberDevices", "/members/:memberId/devices", {
      params: memberParams,
      success: devices,
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      `An organisation administrator lists a person's paired devices, revoked ones included, newest first. 404 for an agent: its key credentials are under /agents/{agentId}/credentials. ${untyped}`,
    ),
    HttpApiEndpoint.delete("revokeMemberDevice", "/members/:memberId/devices/:credentialId", {
      params: { ...memberParams, credentialId: Uuid },
      success: Device,
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      `An organisation administrator revokes one of a person's devices. Its next API request and its next IRC sign-in are refused; the person's other devices keep working. ${cascade} ${untyped}`,
    ),
  )
  .annotate(HttpApi.PayloadParseOptions, { onExcessProperty: "error" })
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const deviceHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof deviceGroup>,
) =>
  handlers
    .handle("listMyDevices", () =>
      handleDb(Effect.flatMap(CurrentMember, (caller) => listDevices(caller, null))),
    )
    .handle("revokeMyDevice", ({ params }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (caller) => revokeDevice(caller, null, params.credentialId)),
      ),
    )
    .handle("listMemberDevices", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (caller) => listDevices(caller, params.memberId))),
    )
    .handle("revokeMemberDevice", ({ params }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (caller) =>
          revokeDevice(caller, params.memberId, params.credentialId),
        ),
      ),
    );
