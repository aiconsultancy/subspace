# Agent key credentials and fleet enrolment

## Intent

A fleet's agents reach the API and IRC with their own identity on any deployment,
including a production API on another host behind proxies, with no cloud key vault
and no secret in a captain's hands. The fleet's gateway (Stargate) holds one
certificate per agent, signs a short-lived token with its key for each request it
forwards to the API's main listener, and signs the agent in to IRC with the same
certificate. An administrator issues a one-time enrolment token for a fleet, and
the gateway registers the fleet's agents and their certificates with it, so the
fleet VM holds no administrator's login.

This record covers tracker issues SUB-9 (agent key credentials on the main
listener) and SUB-10 (fleet enrolment tokens). The contract, which the gateway is
built against, is the API README's
[Agent key credentials](../../../README.md#agent-key-credentials) section.

## Specification

- **Credentials** (`src/agent-keys.ts`, `src/agent-credentials.ts`, migration
  `agent_credentials`). `agent_credentials` holds each agent's certificates:
  organisation, agent, the certificate and its SPKI public key, the algorithm
  (`EdDSA` for Ed25519, `ES256` for P-256), the fingerprint (SHA-256 of the DER in
  lower-case hex, Ergo's certfp), scopes `api` and `irc`, label, creator,
  enrolment token, created, last used and revoked. A partial unique index keeps
  one active credential per fingerprint in the whole database. Administrators list,
  add and revoke an agent's credentials under `/api/v1/agents/{agentId}/credentials`.
- **Agent tokens** (`src/agent-keys.ts`, `src/api.ts`). The bearer middleware sends
  a token whose unverified issuer is `subspace-agent` to the agent token check and
  never to a provider's verifier, and the providers' verifiers refuse that issuer.
  The check follows the README's numbered rules; `AGENT_TOKEN_AUDIENCE` is the new
  setting. Accepted `jti` values go into `agent_token_ids` (credential, jti,
  expiry), which each instance purges every minute of rows that expired more than
  five minutes earlier.
- **Enrolment** (`src/enrolment.ts`, migration `enrolment_tokens`). Admin routes
  under `/api/v1/fleets/{reference}/enrolment-tokens` issue, list and revoke tokens;
  `POST /api/v1/agents/enrol` is authorised by a token alone through its own
  security middleware and creates the agent in the token's fleet, or adds the
  credential to the active agent of that name and rank there.
- **Enrolled agents' subjects** (`src/identity.ts`, `src/registration.ts`,
  `src/agents.ts`). An enrolled agent's `users.subject` is `subspace-agent:` and its
  own user ID. Provider sign-in, first sign-in registration, the membership command
  and agent registration all refuse that form; editing an enrolled agent keeps it.
- **IRC** (`src/irc.ts`). The hook answers a `certfp` with no passphrase from the
  credentials with scope `irc`; `POST /api/v1/me/irc-credential` refuses an agent
  token whose credential lacks `irc`.

## Decisions

- **Signed tokens, not mutual TLS**, as the tracker decided: the API listens on
  plain HTTP behind proxies that do not pass client certificates through reliably.
- **No certificate validity dates.** A self-signed certificate's dates are only what
  its holder asserted, so they prove nothing. Requiring them would end a key's use
  on a date nobody chose, with confusing failures on the API while IRC, which knows
  only the fingerprint, carried on; revocation is the control. Nothing else in the certificate is read: names, extensions and the
  issuer's signature carry no identity here.
- **A fingerprint is unique across organisations among active credentials**, not
  only within one, because an agent token names no organisation and the lookup must
  resolve to one credential by construction. A certificate revoked in an
  organisation cannot be registered there again, so a revoked key stays revoked
  even if an enrolment token is still valid. The cost is that registering another
  organisation's active certificate reveals, by a 409, that it is registered.
- **An agent's IRC nick has at most 32 characters**, a letter then letters, digits,
  `_` or `-`, the account names the IRC image's sign-in script accepts, so that
  every agent that can hold the `irc` scope can sign in. It is checked whenever a
  nick is set, by registration, an edit, enrolment or the username command; nicks
  saved before keep their value until changed.
- **Replay protection in PostgreSQL.** The primary key on credential and `jti`
  makes the first insert win on every instance. The purge keeps a five-minute margin
  beyond each token's expiry, so a difference between the database's clock and an
  instance's cannot let a record go before its token is refused by its expiry.
  Growth is bounded by the tokens accepted in about ten minutes.
- **Organisations whose provider is not configured are not served by agent
  tokens**, as an `AGENT_ORGANISATION` or `IRC_AUTH_ORGANISATION` must name a
  configured provider: a database carried from another deployment does not open
  its agents to whoever holds their old keys.
- **Enrolment uses.** A use is a registration: creating an agent with its
  credential, or adding a credential. The same certificate with the same scopes
  for the same agent again answers 200 with the same IDs, uses nothing and works
  after the uses are spent but not after expiry or revocation, so a gateway can
  retry a lost answer. The token's row is locked for each enrolment, so concurrent
  enrolments never exceed `maxUses`. Defaults: one hour and 16 uses (the installer
  allows a rear admiral and twelve captains); bounds 60 seconds to seven days and
  1 to 100 uses.
- **An existing agent must have the requested rank.** Enrolment adds a credential
  only to the active agent of that name in the token's fleet with the same rank, so
  a gateway that misnames an agent cannot attach a key to a different kind of
  agent. Its other fields are left as they are.
- **The administrator who issued the token registers the agents it creates**, which
  satisfies `registered_by_id` and tells an administrator where an agent came from;
  the audit rows name the token as the actor.
- **Token format.** `senr1.` and base64url of the row's 16-byte ID and 32 random
  bytes. The ID finds the row; only the SHA-256 of the whole text is stored, and it
  is compared in constant time.
- **The IRC scope also governs `sirc1.` credentials** for an agent signed in with a
  key, so an `api`-only key cannot reach IRC by another route.
- **Lock order.** Every transaction and both migrations take the organisation
  before its users, as the username migrations do: enrolment takes the
  organisation's row, then the token's row, then the fleet, ship, user and
  credential; the migrations lock `organisations` first and create the foreign keys
  to `users` before those to `fleets` and before altering `agent_credentials`.

## Trust

An enrolment token lets its holder register agents in one fleet until it expires,
is revoked or is used up; keep it as short-lived as the installation allows. An
agent's private key is its identity on the API and IRC; the gateway user holds
every captain's key, as the tracker accepted. Revoke a credential to end a key's
use at once.

## Known limits

- Registering another organisation's active certificate answers 409, which reveals
  that the fingerprint is registered somewhere.
- `lastUsedAt` is updated at most once a minute and is not exact.
- An agent whose nick of more than 32 characters was saved before this change
  cannot sign in to IRC until an administrator gives it a shorter one.
- The IRC image's README still says to turn on certificate forwarding
  (`IRC_AUTH_CERT_LOGINS`) once the API issues agent certificates, which it now
  does.
- There is no rate limit on agent token checks or enrolment attempts, by the
  owner's rule for this internal system.

## Verification

- `bun run typecheck`, `bun run lint` and `bun test`, with `TEST_DATABASE_URL`
  naming a local PostgreSQL: `tests/agent-keys.test.ts`, `tests/enrolment.test.ts`,
  `tests/irc.test.ts` and `tests/listeners.test.ts` cover every acceptance bullet
  and refusal in the two issues. The tests build their own Ed25519, P-256, P-384 and
  RSA certificates and mint agent tokens as a gateway would, with `NODE_ENV=production`
  and a fictional Entra tenant for people. `tests/agent-keys-migration.test.ts`
  applies the two migrations to a database that already has an organisation, a
  person, a fleet and an agent, twice, and checks the database's own constraints.
- Not verified: the Stargate gateway against this API, a real Ergo with
  certificate forwarding, and the migrations on a copy of a production database.
