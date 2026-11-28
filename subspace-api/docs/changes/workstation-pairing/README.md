# Workstation pairing

## Intent

A person signs in to the API from their own workstation with a key that the
workstation generates and never shares, on a deployment with no identity provider
at all. Root on the API host creates the organisation and its first owner and
prints a single-use invite; the owner's workstation redeems it to register its
certificate, and from then on signs a short-lived token for each request, as a
fleet's gateway does for its agents.

This record covers the first two parts of tracker issue SUB-76, the API contract
that the desktop's sign-in (SUB-77) and setup (SUB-78) are built against: pairing
itself, then [devices, invites, IRC and linked accounts](#part-two-devices-invites-irc-and-linked-accounts). The contract
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
- **Invites** (`src/pairing-invites.ts`, `src/one-time-tokens.ts`, migration
  `pairing_invites`). `pairing_invites` holds the organisation, member, the
  SHA-256 of the invite, expiry, creator, created, redeemed and revoked times.
  The invite's format and hashing are shared with enrolment tokens, which now use
  the same module.
- **Device credentials** (`src/agent-credentials.ts`, `src/agent-keys.ts`).
  Devices reuse `agent_credentials`: a credential belongs to a person or an agent,
  its kind is the member's. The new `pairing_invite_id` is unique, so an invite
  registers at most one credential even if the row lock were bypassed. Device
  credentials had the scope `api` only in this part; part two gives them `irc` as
  well.
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
- **Bootstrap** (`scripts/pairing-owner.ts`, `bootstrapOwner` in `src/pairing-owner.ts`).
  `bun run auth:owner ORG` runs only as root and does everything in one transaction.

## Provision a fleet for setup

On the API host, root runs `bun run --silent auth:provision-fleet`, with the API's
`DATABASE_URL`, `AUTH_PROVIDERS` (including `pairing`), `AGENT_TOKEN_AUDIENCE`,
`DEVICE_TOKEN_AUDIENCE` if the API sets it, and any `RESERVED_USERNAMES`. The
command checks them as the API does before it opens the database, and prints
the setting at fault, never its value. The command reads one UTF-8 JSON object of up to
64 KiB from standard input, or accepts `--file PATH` for a private root-owned
regular file (no group or other permissions; the final path must not be a
symlink). It has no HTTP route. A request looks like this:

```json
{
  "organisation": "example",
  "owner": { "name": "Aubrey Example", "email": "aubrey@example.com" },
  "fleet": {
    "slug": "example",
    "name": "Example Fleet",
    "ircHost": "irc.example.com",
    "ircPort": 6697,
    "ircTls": true,
    "starbaseChannel": "#example-starbase",
    "commandChannel": "#example-command"
  },
  "rearAdmiral": { "name": "atlas", "ircNick": "atlas", "model": "example-model" },
  "captains": [
    {
      "name": "vega",
      "ircNick": "vega",
      "model": "example-model",
      "ship": "Vega",
      "channel": "#vega"
    }
  ],
  "repositories": ["example/repository"]
}
```

- `organisation` names `pairing:ORG`, with the same `ORG` format as
  `auth:owner`. Give `owner.name` on the first run; the e-mail is optional. On
  reruns the existing owner is reused, not renamed or replaced. A `dev:ORG`
  or externally managed organisation with the same suffix is not migrated.
- `fleet` uses the create-fleet API fields, including optional `ircWebsocketUrl`,
  `vmHost` and `fleetApiUrl`. Supply the standard channel names from the installer
  for the fleet and each captain. There is one rear admiral and up to 49
  captains. Officer names, IRC nicks, ships and channels each differ ignoring
  case. Officers are matched by name; same-fleet, same-rank active officers keep
  their member IDs and key credentials. Omission does not delete a ship or
  deactivate an officer. The command does not create Ergo accounts or keys.
- Without an `initiative` field, setup finds a non-archived initiative of the
  fleet's name, preferring one already owned by this fleet over an unassigned
  one. If none matches, it creates one for the fleet. An ambiguous match needs
  an explicit `"initiative": {"id": "<UUID>"}`. Keep the returned ID for
  reruns. An existing initiative of another fleet is moved only with
  `"initiative": {"id": "<UUID>", "adopt": true}`. That includes ownership
  inherited from an ancestor. An unassigned initiative can be attached without
  `adopt`.
- `repositories` is optional (up to 50 `owner/repository` names). Setup reports
  existing, unarchived tracker projects linked to them and their effective fleet,
  including ownership inherited from an ancestor. It does not create, edit or
  move projects. A different `fleetId` is a warning the installer can show.

On success standard output contains one JSON line, with these fields:

| Field                                                  | Shape                                                                                     |
| ------------------------------------------------------ | ----------------------------------------------------------------------------------------- |
| `organisationId`, `ownerId`, `fleetId`, `initiativeId` | UUID strings                                                                              |
| `officers`                                             | Array of `{name, id}` in request order, rear admiral first                                |
| `projects`                                             | Array of `{repository, id, key, fleetId}`; `fleetId` is a UUID or `null`                  |
| `grant`                                                | `{id, token, expiresAt, maxUses}`: UUID, opaque `senr1.` token, UTC timestamp and integer |

Pass only the grant token to Stargate's existing enrolment flow through a private
channel. It is fleet-scoped, valid for ten minutes and has one use per officer in
the specification; as with any fleet grant, a new certificate consumes one use.
The gateway should keep its existing officer keys on reruns. Provision close to
enrolment: each successful run returns a fresh grant; earlier grants keep their
original expiry and use limit. Do not print the response to a setup log or keep
it in setup answers, generated instructions or shell arguments. Only the hash
of the token is stored. On refusal, the command exits non-zero, writes a
diagnostic to standard error and leaves no partial changes or grant. Changes are
audited as API-host CLI actions, not as actions by the owner. Existing records
that require an administrator ID use the owner as their sponsor.

Provisioning mints no owner invite and does not rotate a pending invite or
revoke a device. After setup succeeds, run `auth:owner` separately when an invite
is needed and deliver it only to the owner. No administrator bearer or human key
is needed by setup.

The command runs in one transaction that takes the organisation's row
`FOR UPDATE` first, the mode its initiative writes need, so it never upgrades
that lock while holding fleet, ship or officer rows. Ordinary fleet and ship
creation take the same row before inserting, as other writes to an
organisation's members and agents do. A request that overlaps a run waits for
it and then commits or gets the usual 409; neither deadlocks.

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

## Known limits of part one

Part two below resolves the first three.

- Administrators could not issue invites for other members, or list and revoke a
  member's devices, through the API; bootstrap paired only the owner.
- Device credentials had no `irc` scope, so a paired person could not sign in to
  IRC. When they could, revoking a device would have to stop fresh IRC sign-ins,
  including with an IRC password issued before the revocation.
- A paired-only member could not be linked to an identity provider account.
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

## Part two: devices, invites, IRC and linked accounts

### Intent

An organisation that starts with pairing grows and changes without root on the
host: an administrator invites people and pairs their further workstations, anyone
removes a lost workstation of theirs without touching their others, a paired person
uses IRC, and the organisation can turn on WorkOS or Entra later without anyone
becoming a new member. The contract is in the API README:
[Invites](../../../README.md#invites),
[Devices](../../../README.md#devices),
[IRC with a paired device](../../../README.md#irc-with-a-paired-device) and
[Linking a provider account](../../../README.md#linking-a-provider-account).
Development sign-in is unchanged; its removal is a later part of SUB-76.

### Specification

- **Invites** (`src/invites.ts`; `insertPairedPerson`, `issueInvite` and their
  audit fields in `src/pairing-invites.ts`). `POST /api/v1/pairing/invites` takes
  `memberId` or a new person's `displayName`, `email` and `role`, and an optional
  lifetime; `POST /api/v1/me/pairing/invites` takes a lifetime only and pairs
  another device of the caller. Both share person creation, invite issue, the
  revocation of the person's pending invites and their audit fields with
  bootstrap, so all behave alike. A new person gets a `subspace-member:` subject
  and a username, and is audited as `memberRegistered`. `memberId` names only an
  active person with a `subspace-member:` subject, no linked account and no
  unrevoked device; anyone else is refused as
  `urn:subspace:problem:member-signs-in`. Each invite records its issuer in
  `created_by_id` and, for a device-signed request, the device in
  `created_by_credential_id`. Issue runs under the organisation's lock and reads
  the caller's membership, role and signing device again there (`confirmCaller`
  in `src/people.ts`, which the device and linked account routes share with the
  person a request manages). The role read again comes from the source the
  request signed in with (`RoleSource`); for a person's own Entra token it is
  the recorded token role but never above the role that request's token granted
  (`tokenGrant`, `requestRole` in `src/identity.ts`), so a withdrawal another
  sign-in records meanwhile applies and a grant does not. Both routes answer
  `pairing-unavailable` when pairing is off, because no device token could be
  checked. Administrators list pending invites and revoke one
  (`GET /api/v1/pairing/invites`, `DELETE /api/v1/pairing/invites/{inviteId}`).
  The list leaves out an invite that redemption would refuse for its issuer, by
  the same rule (`issuerWithdrawal` in `src/pairing-invites.ts`).
- **Devices** (`src/devices.ts`). `/api/v1/me/devices` and
  `/api/v1/members/{memberId}/devices` list and revoke key credentials of a person,
  never of an agent, and the agent credential routes still never show a device. A
  device shows who issued the invite that paired it (`invitedByUserId`,
  `invitedByDeviceId`). Revocation locks the organisation, is idempotent, and is
  audited once as `deviceCredentialRevoked`. It walks what the device issued: its
  pending invites, the devices paired with its invites and, in turn, theirs,
  revoking and auditing each, so redemption, which takes the same lock, never pairs
  with an invite whose issuing device is revoked.
- **Issuers who lose the right** (`provisionMember` and `applyTokenRole` in
  `src/registration.ts`). Deactivating a member or taking the
  administrator role from them, by the membership command or an Entra token's
  role, revokes the pending invites they issued for other people. Both take the
  organisation's row before the member's and the invites' rows, as redemption
  does, so an issuer's sign-in that withdraws their role waits for a redemption
  of their invite rather than deadlocking with it. An invite for
  another person also records where the issuer's administrator role came from
  (`issuer_authority`: the provisioned role whenever it made them one, otherwise
  their Entra token's app role; `issuerAuthorityOf` and `issuerRoleSource` in
  `src/identity.ts` map it to and from a `RoleSource`), and redemption
  (`issuerRefusal` in `src/pairing-invites.ts`) checks under the lock that the
  issuer is still active and an administrator from that source under the current
  `ENTRA_ADMIN_ROLE`, and that the device that issued any invite is unrevoked.
  Removing the setting thus withdraws the invites only its role issued at once,
  with no one signing in, and leaves those of an administrator by both roles.
  The provisioned role is judged as a paired device's is, never above the
  recorded token role while the setting is configured, so no invite acts above
  what the issuer could do from any sign-in now.
- **Bound IRC passwords** (`src/irc.ts`). A password issued to a request signed
  with a key credential is `sirc2.`: the `sirc1.` body followed by the credential's
  UUID, under a MAC that also covers the prefix. On every sign-in the hook checks
  that the credential is unrevoked, belongs to the password's user in the hook's
  organisation and has the scope `irc`, and records its use. Device credentials
  have the scopes `api` and `irc`; the migration gives existing ones `irc`. A
  device's certificate still never signs in to IRC by itself: certificate sign-in
  stays for agents.
- **Linked accounts** (`src/linked-identities.ts`, `findSignedInMember` in
  `src/identity.ts`, migration `member_identities`). `member_identities` holds a
  member's WorkOS or Entra account: organisation, member, provider, provider
  organisation and subject. `member_identities_account` makes an account unique in
  the whole database, and `member_identities_one_per_provider` gives a member at
  most one account of each provider. A provider sign-in looks for the member
  registered under the account first, as before, then for the member that linked
  it.
- **Linking** (`POST /api/v1/me/identities`). The request is signed by a paired
  device and carries the account's access token, which the API checks with the
  same verifier as a sign-in and which must have been issued in the last ten
  minutes. In one transaction it reads the provider organisation's claim
  (`providerOrganisationClaim` in `src/provider-organisations.ts`), which takes an
  advisory lock on the provider organisation, then locks the caller's
  organisation's row, checks the rules below, inserts the link and audits
  `memberIdentityLinked` with the device's credential ID. Members list and unlink
  their own links, and administrators anyone's (`memberIdentityUnlinked`).
- **Registration** (`src/registration.ts`). First sign-in reads the same claim,
  under the same lock, and registers no one of a provider organisation that is
  linked into another organisation. A link found under that lock, committed after
  sign-in's own lookup, is returned as a link, so the token's administrator role
  is not applied to it and a link into another organisation is not refused as a
  bound tenant. The membership command reads the same claim, refuses to register a
  linked account, and adds no one to a bound provider organisation's own
  organisation.
- **Problem types**. `urn:subspace:problem:identity-token-unusable` (400) and
  `urn:subspace:problem:identity-conflict` (409), named in full in the OpenAPI
  description. The typed refusals of the signed-in routes (these two,
  `pairing-unavailable` and `member-signs-in`) have their constructors in
  `src/problems.ts`, beside `problemTypes`. These routes keep the shared
  validation problem for a malformed body, with its field detail, rather than
  redemption's `invalid-request`, and each route's OpenAPI description names
  which of its problems carry a type; the rest are `about:blank`.
- **Unique violations inside a transaction** (`hasConstraint` in `src/fleets.ts`)
  are now found inside an Effect cause too, where a failed query carries them, so
  they answer 409 rather than 500.

### Decisions

- **One credential model for devices and agents' keys, so one IRC binding.** An
  agent's IRC password is bound to its key credential in the same way, so revoking
  an agent's key now also refuses a password it already holds. People signed in
  through a provider keep stateless `sirc1.` passwords: they have no credential to
  bind to, and their membership is checked at each sign-in as before.
- **The hook checks the credential on every new sign-in, not on open
  connections.** Ergo asks only at SASL time, so an established connection may run
  to its end; the change only refuses the next sign-in.
- **No new IRC password storage.** The binding is in the signed password, so
  passwords stay stateless and the hook needs one indexed lookup.
- **A link needs both halves in one request.** The device's signature proves the
  member and the provider token proves the account, so neither a stolen provider
  token nor a stolen device alone links anything, and no one can link an account to
  someone else. The ten-minute limit on the token's age narrows the use of a leaked
  but unexpired token. Linking is not offered from a provider sign-in, so a
  provider session can never attach itself to a member it is not.
- **An account names at most one member.** The unique constraint across the whole
  database, the refusal of an account already registered as a member, the
  membership command's refusal of a linked account, and sign-in's order (registered
  first, then linked, which never both match) mean an account can never sign in as
  two members, nor change member silently.
- **An administrator's invite never takes over a person who signs in.** A
  `memberId` invite gives whoever redeems it a device of that person, with their
  authorship, IRC account and administrator rights, and the device can then link a
  provider account. It is therefore refused for a person who signs in some other
  way; they pair further devices themselves, from a provider sign-in or a device.
  The administrator is still trusted with new people and with people who have no
  way to sign in left, whom they could equally have created. Other options were
  weighed: barring invites for administrators only still exposes members, and
  notifying the person is out of reach for someone with no device left.
- **A device's issue goes with it.** A stolen workstation could otherwise pair a
  device of its own, by its holder's own invite or, for an administrator, a new
  person's, and keep access after it is revoked. The issuing device is recorded on
  the invite, and revocation follows that record transitively. Revoking an
  administrator's device thus also revokes the people it onboarded; they pair again.
  The owner bootstrap command's `--revoke-devices` walks it too, with the same
  code, in its own transaction and audited as the command, so a recovered owner
  finds no device or invite of the thief's left to act before they sign in. A
  provider account linked from the stolen device is the member's and stays linked;
  the `memberIdentityLinked` event names the device it was linked from.
- **A provider organisation belongs to one Subspace organisation.** Otherwise one
  Entra tenant's people could split across two organisations, or an account could
  be linked in an organisation while its own organisation already holds its
  colleagues. Linking into another organisation is refused while the provider
  organisation's own organisation has members or the provider organisation is
  linked elsewhere, and, once it is linked, its unknown people are not registered
  in their own organisation, nor added to it by the membership command. An advisory
  lock per provider organisation, taken before any row lock, serialises linking,
  registration and the membership command, so the checks cannot race.
- **An administrator opens the organisation to a provider organisation.** The
  first link from a provider organisation into an organisation other than its own
  lets any of its account holders with a paired device link there, so it needs an
  administrator. Linking within the provider organisation's own organisation needs
  no administrator.
- **A linked account signs in with the member's own role.** An Entra token's
  administrator role (`ENTRA_ADMIN_ROLE`) describes the tenant's own organisation;
  it is not applied to a member of another organisation who linked an account.
- **Unlinking is immediate and leaves the rest.** An unlinked account signs in to
  the API as no one from its next request (or, in its own provider organisation
  with registration on, is registered afresh there as a new person); the member's
  devices and other accounts keep working. It does not reach IRC passwords already
  issued: a `sirc1.` password the account obtained while linked is bound to no key
  credential, so it still signs in to IRC as the member until it expires, ten
  minutes after issue. Unlinking adds no state that the hook could check, and the
  window is that short.

### Migration

`drizzle/20261127021150_member_identities` locks `organisations`, creates
`member_identities` with its unique constraints, its provider check and its foreign
keys to the organisation and the member, and gives existing device credentials
(those registered with a pairing invite) the scopes `api` and `irc`. Agents'
credentials are untouched. `drizzle/20261127034930_invite_issuing_device` adds the
nullable `pairing_invites.created_by_credential_id`, with an index and a foreign
key to a key credential of the same organisation (which adds the unique
`agent_credentials_scope_id`). Existing invites name no device, so revoking a
device reaches only what it issues from this change on.
`tests/linked-accounts-migration.test.ts` applies both to a part-one database with
data, and again. `drizzle/20261127160426_invite_issuer_authority` adds one
nullable column, `pairing_invites.issuer_authority`, with no default and no
backfill. Existing invites keep null, which redemption judges as it does the
provisioned role. It takes only the table lock that adding a column needs,
briefly, and no organisation lock, since it reads and writes no rows.

**Upgrade the IRC image with or before this API.** This API issues `sirc2.`
passwords to every key credential, agents' included, and an IRC image whose
`subspace-auth` script forwards only `sirc1.` refuses them locally, without asking
the API. From this change the script forwards both prefixes by default; an
`IRC_AUTH_PASSPHRASE_PREFIX` that is set replaces that list, so a server that
sets it must add `sirc2.` (start-up warns when it lists `sirc1.` without `sirc2.`).
The new script works with an older API, which never issues `sirc2.`, so upgrading
IRC first is always safe.

- Guided setup (`subspace-setup`) builds both from the same payload in one run. It
  brings the API up first (`api_up`), then runs the fleet installer, which rebuilds
  and restarts Ergo from the payload's `subspace-irc`. Between the two, a client
  that fetches a new password gets `sirc2.` and is refused; it signs in on its next
  retry once Ergo has restarted, as with any refused sign-in. Run the setup to
  completion rather than stopping after the API.
- A shared IRC server (`subspace-setup irc-server`) or an IRC server managed
  separately has its own upgrade. Upgrade it, from the same revision or later,
  before any API it serves.

#### Cutting over from part one

The image runs its migrations at start-up, and the scope backfill runs once: once
the migration is recorded as applied, it never runs again. A part-one instance
still serving after the migration redeems invites as it always has, giving the new
device `["api"]` only, so that device cannot get an IRC password. Cut over in one
step rather than rolling instance by instance:

1. Upgrade the IRC image, as above.
2. Stop every part-one API instance.
3. Start the part-two image, which migrates before it serves.

If a part-one instance did serve after the migration, give the devices their
scopes again. The statement is safe to run more than once and leaves agents'
credentials alone:

```sql
UPDATE agent_credentials SET scopes = array['api', 'irc']::text[]
WHERE pairing_invite_id IS NOT NULL AND scopes <> array['api', 'irc']::text[];
```

#### Rolling back to part one

Part one's [Rolling back](#rolling-back) still applies to an image without
pairing. The part-one image keeps pairing, and its configuration, working, but it
knows nothing of `member_identities`. A linked account no longer signs in as its
member, and the image neither refuses registration in a bound provider
organisation nor checks links in the membership command. The IRC image need not
be rolled back: its script works with either API.

1. **Stop new registrations.** If `member_identities` has any rows, remove `workos`
   and `entra` from `AUTO_REGISTER` (or leave it unset) on the part-one image, and
   do not run its membership command for a person of a linked provider
   organisation. Otherwise a linked account's first sign-in registers it afresh in
   its provider organisation's own Subspace organisation, and once part two is back
   that registration wins over the link, since sign-in tries the registered member
   first. A bound provider organisation's unknown people would be registered in an
   organisation of their own too, splitting its people.
2. **Stop every part-two instance**, so that none redeems an invite with `irc`.
3. **Withdraw part two's invites.** The part-one image redeems an invite without
   checking that its issuer is still an administrator from the source they issued
   it with, or that the device that issued it is unrevoked. Revoke every pending
   invite a member issued; the owner bootstrap command's stay:

   ```sql
   UPDATE pairing_invites SET revoked_at = clock_timestamp()
   WHERE created_by_id IS NOT NULL AND redeemed_at IS NULL AND revoked_at IS NULL;
   ```

   While part one runs, revoking a device, as `--revoke-devices` does, revokes that
   device only: the devices paired with its invites keep signing in. After a stolen
   workstation, revoke them too, or upgrade again and revoke the stolen device
   through the device routes, which revoke what a revoked device issued.

4. **Take `irc` back from the devices.** The part-one image would give a device
   with `irc` an unbound `sirc1.` password, which revoking the device does not
   refuse until it expires:

   ```sql
   UPDATE agent_credentials SET scopes = array['api']::text[]
   WHERE pairing_invite_id IS NOT NULL;
   ```

5. **Start the part-one image.** The links stay stored, unused.

#### Upgrading again after a rollback

Cut over as above: stop every part-one instance, then start part two. The
migration is recorded as applied, so it does not restore the device scopes: run
the statement under [Cutting over from part one](#cutting-over-from-part-one)
once part two is up. Before letting people sign in, reconcile what the part-one
image may have registered:

- Accounts that are both linked and registered as a member. Each signs in as the
  registered member, not the one that linked it:

  ```sql
  SELECT u.id AS registered_member, u.created_at, i.user_id AS linked_member, i.id AS link
  FROM member_identities i
  JOIN organisations o ON o.provider = i.provider AND o.external_id = i.external_organisation
  JOIN users u ON u.organisation_id = o.id AND u.subject = i.subject;
  ```

- Members of a provider organisation's own Subspace organisation while its accounts
  are linked into another one. Part two never creates these, and while any exist no
  further account of that provider organisation can be linked:

  ```sql
  SELECT u.id, u.subject, u.created_at, o.provider, o.external_id
  FROM users u
  JOIN organisations o ON o.id = u.organisation_id
  WHERE EXISTS (
    SELECT 1 FROM member_identities i
    JOIN organisations l ON l.id = i.organisation_id
    WHERE i.provider = o.provider AND i.external_organisation = o.external_id
      AND NOT (l.provider = o.provider AND l.external_id = o.external_id)
  );
  ```

For each row, decide which member the person keeps. To keep the new membership,
unlink the account (`DELETE /api/v1/members/{memberId}/identities/{identityId}`).
To keep the link, delete the new member (`DELETE FROM users WHERE id = …`).
Deactivating it is not enough, as an inactive registered member still takes
precedence and still counts as a member. The delete takes the member's initiative
memberships, subscriptions, notifications and saved views with it, and leaves the
audit log as it is. It fails on a foreign key if the member wrote anything, such
as an issue or a comment: then decide what happens to that work first, or keep the
new membership instead.

### Known limits

- Registration in a provider organisation linked into another organisation is
  refused, not redirected: an unknown person of it signs in only after an
  administrator invites them and they link from a paired device.
- Any administrator of any organisation who holds an account of a WorkOS
  organisation, and a device, can bind that organisation to theirs while its own
  Subspace organisation has no members, as for an Entra tenant's member. Its
  unknown people are then refused at first sign-in until the binding is released.
  Entra guests cannot bind a tenant, nor can a token without the optional `acct`
  claim, 0, which alone tells a member from a guest whose home tenant is this one;
  WorkOS has no such notion. Binding only by the
  operator would close this, and is left for later. Until then, the operator lists
  bindings and releases a wrong one with `bun run auth:bindings`
  (`scripts/provider-bindings.ts`, `src/provider-organisations.ts`), audited.
- Administrator rights that come only from `ENTRA_ADMIN_ROLE` do not carry over to
  a paired device or a linked account. An Entra member who is an administrator by
  that role alone acts with their stored role, the one the membership command or
  an invite set, when they sign in from a device or through a linked WorkOS
  account, because neither carries an Entra role. Nor, while the setting is
  configured, does a stored administrator role whose holder's token no longer
  carries the app role, once an Entra sign-in has recorded that. A withdrawal in
  Entra reaches the API only with the person's next Entra sign-in.
- A link needs a paired device, so a member who signs in only through a provider
  pairs one first, with their own invite from `/api/v1/me/pairing/invites`.
- An administrator can still create a person, or recover one with no way to sign
  in, and redeem that invite themselves. The audit log records who issued every
  invite, and each device shows its issuer.
- Revoking a device still does not close an IRC connection or update stream
  already open.
- There is no rate limit on redemptions or link attempts.

### Verification

- `bun run typecheck`, `bun run lint` and `bun test` with `TEST_DATABASE_URL`.
  `tests/devices.test.ts` runs a production-mode API with Entra, WorkOS and pairing
  on, and covers administrator invites for new people and for people with no way to
  sign in, the refusal of people who sign in, a person's own invites, their
  replacement, refusals and absence from storage, audit and logs; the cascade from
  a revoked device through the devices and invites it issued; listing and revoking
  pending invites; the caller checked again under the lock; listing and
  revoking devices, by the person and by an administrator, with the other devices
  still working; a device's `sirc2.` password accepted by the hook and refused once
  the device is revoked; Entra and WorkOS accounts linked from a device and signing
  in as the same member with the same username and IRC account; and the refusals:
  an account linked or registered elsewhere, a second account of one provider, a
  stale, foreign or malformed token, a provider sign-in or a non-administrator's
  first link, another organisation linking a bound tenant, registration in a bound
  tenant, the membership command on a linked account, and unlinking.
  `tests/provider-bindings.test.ts` covers an Entra guest, a foreign home tenant
  and a token without `acct` refused as a tenant's first link, a member's own link
  without `acct` once it is bound, and the operator listing and releasing a binding.
  `tests/invite-issuers.test.ts` covers the revocation of a demoted or deactivated
  administrator's invites. `tests/invite-authority.test.ts` covers redemption's
  check of the issuer: `ENTRA_ADMIN_ROLE` removed, the issuing device revoked
  outside the walk, the rollback statement, and an issuer's sign-in that
  withdraws their role while a redemption of their invite holds it, an
  administrator by both roles, and the pending list leaving out what redemption
  refuses.
  `tests/irc.test.ts` covers the `sirc2.` format and the hook's credential checks.
- Not verified: the desktop against these routes, a real WorkOS or Entra tenant,
  and the migration on a copy of a production database.
