# Workstation pairing

## Intent

A person signs in to the API from their own workstation with a key that the
workstation generates and never shares, on a deployment with no identity provider
at all. Root on the API host creates the organisation and its first owner and
prints a single-use invite; the owner's workstation redeems it to register its
certificate, and from then on signs a short-lived token for each request, as a
fleet's gateway does for its agents.

This record covers the first part of tracker issue SUB-76, the API contract that
the desktop's sign-in (SUB-77) and setup (SUB-78) are built against. The contract
itself is the API README's
[Workstation pairing](../../../README.md#workstation-pairing) section. The
[fixture](device-token-fixture.json) is a synthetic Ed25519 key, its certificate
and fingerprint, and a device token with its header and claims, valid only at
`verifyAt`, for client tests. Its key is public test data.

## Specification

- **Provider** (`src/config.ts`, `src/schema.ts`). `organisations.provider` and
  `AUTH_PROVIDERS` accept `pairing`. It has no external sign-in: its members sign
  in only with device tokens. With `pairing` configured the API needs
  `DEVICE_TOKEN_AUDIENCE` or `AGENT_TOKEN_AUDIENCE`, as an `https` origin unless
  its host is loopback.
- **Paired-only members** (`src/identity.ts`). A member created for pairing has the
  subject `subspace-member:` and its own user ID. No provider issues one, so, like
  an enrolled agent's `subspace-agent:` subject, provider sign-in, first sign-in and
  the membership command never match or register it.
- **Invites** (`src/pairing.ts`, `src/one-time-tokens.ts`, migration
  `pairing_invites`). `pairing_invites` holds the organisation, member, the
  SHA-256 of the invite, expiry, creator, created, redeemed and revoked times.
  The invite's format and hashing are shared with enrolment tokens, which now use
  the same module.
- **Device credentials** (`src/agent-credentials.ts`, `src/agent-keys.ts`).
  Devices reuse `agent_credentials`: a credential belongs to a person or an agent,
  its kind is the member's. The new `pairing_invite_id` is unique, so an invite
  registers at most one credential even if the row lock were bypassed. Device
  credentials have the scope `api` only.
- **Device tokens** (`src/agent-keys.ts`, `src/api.ts`). The key token check takes
  a map from `typ` to issuer, audience and member kind: `subspace-agent+jwt` with
  `subspace-agent` for agents, `subspace-device+jwt` with `subspace-device` for
  people. The bearer middleware sends either issuer to that check and never to a
  provider's verifier. Replay protection, revocation and the provider check are the
  agent tokens'.
- **Redemption** (`src/pairing.ts`). `POST /api/v1/pairing/redeem` has no bearer
  middleware: the invite in the body is its authorisation. In one transaction it
  locks the organisation, then the invite's row, checks the hash in constant time,
  that the invite is unspent and that its member is an active person of a served
  organisation, inserts the credential, marks the invite redeemed and audits both.
- **Probe** (`src/pairing.ts`). `GET /api/v1/pairing` returns whether pairing is on
  and the device token profile, with no token.
- **Problem types** (`src/problems.ts`, `src/middleware.ts`). The pairing routes'
  refusals carry stable `urn:subspace:problem:` types; the rest of the API keeps
  `about:blank`.
- **Bootstrap** (`scripts/pairing-owner.ts`, `bootstrapOwner` in `src/pairing.ts`).
  `bun run auth:owner ORG` runs only as root and does everything in one transaction.

## Decisions

- **Reuse the agent key model, not a new credential type.** A device credential is
  an agent credential that belongs to a person: one table, one certificate parser,
  one replay table and one verifier, so the two cannot drift. Officer credentials,
  their scopes and the agent routes are unchanged, except that the agent credential
  routes now find only agents, so they cannot list or revoke a person's device.
- **A separate `typ` and `iss` for devices**, so a token says which kind of member
  it signs in, and a person's key cannot sign in as an agent nor an agent's as a
  person, even with a mistaken credential row.
- **A separate, pinned audience.** People's workstations may reach the API by a
  private name that agents do not use. `DEVICE_TOKEN_AUDIENCE` is configuration
  and falls back to `AGENT_TOKEN_AUDIENCE`; it is never taken from `Host`, and no
  origin check is relaxed.
- **An `https` device audience.** Workstations send invites to the device
  audience, and pinning it does not protect an invite sent in plain text, so the
  API refuses to start with pairing on and a plain `http` device audience, even
  one that falls back to the agent audience. Only a loopback host may use `http`,
  for development. TLS may end at a trusted proxy in front of the API.
- **No lock for a wrong secret.** Redemption compares the invite's hash in
  constant time on an unlocked read and refuses a mismatch there; only a matching
  invite takes the organisation's and the invite's locks, where it is checked
  again. Someone who knows an invite's row ID but not its secret cannot learn from
  a lock wait that the row exists, nor hold up real writes.
- **Ed25519 is the interoperable algorithm**, listed first in the probe; P-256 is
  accepted as for agents.
- **One refusal for every unusable invite.** Malformed, unknown, wrong secret,
  expired, revoked, used, or a member who can no longer pair all answer the same
  401 and detail, so the answer tells nothing about which invites exist. The reason
  is logged at debug level; the invite never is.
- **Reconcile before replacing a pending device.** A retry with the same invite
  returns 401 if the first attempt committed. While the invite may still be
  usable, keep the pending key and certificate, retry with the same invite and
  certificate, and check with a freshly signed `GET /api/v1/me`. A 200 confirms
  pairing; even a second `/me` 401 does not rule out an in-flight commit. Ask for
  a new invite only after the API refuses the original invite as unusable and
  a fresh `/me` still returns 401 with the correct audience and clock, or after
  an administrator confirms the original invite cannot be redeemed and the
  pending device is not active. Asking for a replacement invite never permits
  discarding a pending key whose outcome is still unknown; the replacement pairs
  a new key. Otherwise keep the pending record for later reconciliation. No
  separate lookup by certificate is needed.
- **A 400 or 409 does not spend the invite**, so a client bug or a reused
  certificate does not cost the person their invite.
- **The newest invite wins.** Each bootstrap run revokes the owner's pending
  invites, so at most one invite for the owner is usable at a time.
- **Recovery is bootstrap with `--revoke-devices`**: root on the host can always
  regain the owner's access, and the lost workstation's key stops working at its
  next request. It revokes all the owner's devices, because a per-device list is
  not yet exposed.
- **The owner is found, not named**: the organisation's earliest administrator with
  a paired-only subject. A deactivated owner is refused unless host root passes
  `--reactivate`, which restores the same member and records an audit event.
  An owner may have been deactivated because a device was compromised, so if any
  of their device credentials was never revoked, reactivation also needs
  `--revoke-devices` or `--keep-devices`, never both. The audit event records
  the choice, the number of devices and their credential IDs.
- **Invite lifetime** defaults to one hour, within five minutes to one day.
  Redemption judges expiry by the database's clock once it holds the
  organisation's lock, never by the API process's, so replicas with skewed
  clocks agree: after one refuses an invite as expired, no later redemption
  pairs with it.
- **Invite format.** `spair1.` and the unpadded base64url encoding of the row's
  16-byte ID and 32 random bytes, like `senr1.` enrolment tokens. The prefix is the
  version.
- **Lock order.** Redemption and bootstrap take the organisation's row before the
  invite, member and credential rows, as enrolment does; the migration locks
  `organisations` before creating the foreign keys.

## Migration

`drizzle/20261127005123_pairing_invites` creates `pairing_invites` and adds
`agent_credentials.pairing_invite_id`, nullable, with a unique index and a foreign
key to the invite. Existing rows are untouched and need no backfill. The provider
value `pairing` needs no migration: `organisations.provider` has no check
constraint.

### Rolling back

The schema change is additive, so an earlier image runs on the migrated database
and leaves the table and column unused. Rolling back the image does not roll back
sign-in, though:

- The earlier image refuses to start while `AUTH_PROVIDERS` contains `pairing`
  (`AUTH_PROVIDERS must contain workos, entra or dev`), so the rollback must remove
  `pairing` from it as well. It may also remove `DEVICE_TOKEN_AUDIENCE`, which the
  earlier image ignores.
- In an install with other providers, such as `AUTH_PROVIDERS=entra,pairing` rolled
  back to `AUTH_PROVIDERS=entra`, members of the other providers' organisations sign
  in as before. Members of a pairing organisation cannot sign in at all until
  pairing is back.
- In a pairing-only install, nobody can sign in until an image that pairs devices is
  deployed again with `pairing` restored to `AUTH_PROVIDERS`. Plan such a rollback as
  an outage, or roll forward instead.

Restoring a pairing-capable image and the setting brings every paired device back
as it was: rollback revokes nothing, and the devices' credentials stay stored.

## Known limits, for the next part of SUB-76

- Administrators cannot yet issue invites for other members, or list and revoke a
  member's devices, through the API; bootstrap pairs only the owner.
- Device credentials have no `irc` scope, so a paired person cannot sign in to IRC
  yet. When they can, revoking a device must also stop fresh IRC sign-ins,
  including with an IRC password issued before the revocation.
- A paired-only member cannot be linked to an identity provider account; a later
  change would add that link.
- Revoking a device does not close an update stream already open, which ends
  within three minutes when its token expires.
- There is no rate limit on redemptions; an invite has 256 bits of secret.

## Verification

- `bun run typecheck`, `bun run lint` and `bun test`, with `TEST_DATABASE_URL`
  naming a local PostgreSQL. `tests/pairing.test.ts` runs the API in-process with
  `NODE_ENV=production`, `AUTH_PROVIDERS=entra,pairing` and distinct agent and
  device audiences, and covers bootstrap on a fresh database and a rerun;
  redemption and a signed `/api/v1/me` and `/mcp` call as the owner, with Ed25519
  and P-256; used, expired, revoked, unknown and malformed invites; concurrent
  redemptions; a wrong secret, which takes no lock and leaves the invite usable;
  an `http` device audience off loopback; a retry after commit; replayed,
  misdirected, over-long and
  cross-kind tokens; a revoked device; a deactivated owner; unpaired callers with
  headers or forged tokens; the update stream ending at token expiry; the probe on
  and off; the problem types; audit rows and logs free of invites; and the fixture.
  `tests/agent-keys.test.ts` and `tests/enrolment.test.ts` still pass unchanged.
- Not verified: `auth:owner` as root on a real host (its logic is tested through
  `bootstrapOwner`), the desktop against this API, and the migration on a copy of a
  production database.
