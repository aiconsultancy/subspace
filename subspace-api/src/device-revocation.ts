import { and, eq, inArray, isNull } from "drizzle-orm";
import { Effect } from "effect";
import { type AuditFields } from "./audit";
import { Database } from "./db";
import { agentCredentials, pairingInvites } from "./schema";

/**
 * Revoking people's devices, the one way both the device routes and the owner bootstrap command
 * on the API host do it: a revoked device's tokens are refused from its next request and its IRC
 * passwords from its next IRC sign-in, and what it issued is revoked with it, so that it leaves no
 * foothold through an invite it issued or a device it paired.
 */

type Db = (typeof Database)["Service"];

/**
 * Who revokes devices: their organisation, the member recorded as revoking them, or null for a
 * command on the API host, and where each audit row goes, under that actor.
 */
export interface DeviceRevoker<E> {
  readonly organisationId: string;
  readonly memberId: string | null;
  readonly audit: (fields: AuditFields) => Effect.Effect<unknown, E>;
}

/** A device to revoke, as its row stands under the organisation's lock. */
export interface RevokedDevice {
  readonly id: string;
  readonly userId: string;
  readonly fingerprint: string;
  readonly revokedAt: Date | null;
}

/** What a revocation revoked besides the devices it was asked to. */
export interface Cascade {
  readonly devices: number;
  readonly invites: number;
}

/**
 * Revokes what the devices `issuers` issued: their invites still pending, and the devices paired
 * with their invites, then in turn what those devices issued, auditing each. A device paired with
 * an invite was paired after the device that issued it, so the walk ends. Devices revoked earlier
 * are walked too, so that a revocation that did not reach what they issued is completed.
 */
const revokeIssued = <E>(
  tx: Db,
  revoker: DeviceRevoker<E>,
  issuers: readonly string[],
  now: Date,
) =>
  Effect.gen(function* () {
    const inOrganisation = eq(pairingInvites.organisationId, revoker.organisationId);
    let cascade: Cascade = { devices: 0, invites: 0 };
    let frontier = issuers;
    while (frontier.length > 0) {
      const invites = yield* tx
        .update(pairingInvites)
        .set({ revokedAt: now })
        .where(
          and(
            inOrganisation,
            inArray(pairingInvites.createdByCredentialId, frontier),
            isNull(pairingInvites.redeemedAt),
            isNull(pairingInvites.revokedAt),
          ),
        )
        .returning();
      for (const invite of invites)
        yield* revoker.audit({
          action: "pairingInviteRevoked",
          entityType: "PairingInvite",
          entityId: invite.id,
          summary: "Revoked a pending pairing invite: the device that issued it was revoked.",
          changes: { memberId: invite.userId, issuingDeviceId: invite.createdByCredentialId },
        });
      const paired = yield* tx
        .select({
          id: agentCredentials.id,
          revokedAt: agentCredentials.revokedAt,
          issuingDeviceId: pairingInvites.createdByCredentialId,
        })
        .from(agentCredentials)
        .innerJoin(pairingInvites, eq(agentCredentials.pairingInviteId, pairingInvites.id))
        .where(and(inOrganisation, inArray(pairingInvites.createdByCredentialId, frontier)))
        .for("update", { of: agentCredentials });
      let devices = 0;
      for (const { id, revokedAt, issuingDeviceId } of paired) {
        if (revokedAt) continue;
        const [row] = yield* tx
          .update(agentCredentials)
          .set({ revokedAt: now, revokedById: revoker.memberId })
          .where(eq(agentCredentials.id, id))
          .returning();
        yield* revoker.audit({
          action: "deviceCredentialRevoked",
          entityType: "DeviceCredential",
          entityId: id,
          summary: "Revoked a device paired with an invite that a revoked device issued.",
          changes: { memberId: row!.userId, fingerprint: row!.fingerprint, issuingDeviceId },
        });
        devices += 1;
      }
      cascade = { devices: cascade.devices + devices, invites: cascade.invites + invites.length };
      frontier = paired.map((row) => row.id);
    }
    return cascade;
  });

/**
 * Revokes `devices` of the revoker's organisation and what they issued (see `revokeIssued`), in a
 * transaction that holds the organisation's lock, dated `now`. Each device not revoked already is
 * audited with `summary`, and with `cascade`, what this revocation revoked with it; a device
 * revoked already stays as it is, after anything it issued is revoked. Returns `cascade`.
 */
export const revokeDevices = <E>(
  tx: Db,
  revoker: DeviceRevoker<E>,
  devices: readonly RevokedDevice[],
  summary: (device: RevokedDevice) => string,
  now: Date,
) =>
  Effect.gen(function* () {
    const fresh = devices.filter((device) => !device.revokedAt);
    if (fresh.length)
      yield* tx
        .update(agentCredentials)
        .set({ revokedAt: now, revokedById: revoker.memberId })
        .where(
          and(
            eq(agentCredentials.organisationId, revoker.organisationId),
            inArray(
              agentCredentials.id,
              fresh.map((device) => device.id),
            ),
          ),
        );
    const cascade = yield* revokeIssued(
      tx,
      revoker,
      devices.map((device) => device.id),
      now,
    );
    for (const device of fresh)
      yield* revoker.audit({
        action: "deviceCredentialRevoked",
        entityType: "DeviceCredential",
        entityId: device.id,
        summary: summary(device),
        changes: { memberId: device.userId, fingerprint: device.fingerprint, cascade },
      });
    return cascade;
  });
