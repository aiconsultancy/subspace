# Sign in once: registration, username and IRC from one login

## Intent

A new person installs the desktop, signs in with their organisation's identity
provider, and can use the tracker and IRC straight away. Nobody runs a command,
registers an IRC account or sends a password. The API is the only identity
authority: Ergo never talks to an identity provider, but asks the API through its
auth-script hook.

This record covers the API's part: three feature commits and the fixes from the
first review round. The IRC image and the desktop build on the contract in the
API README's [IRC sign-in](../../../subspace-api/README.md#irc-sign-in) section.

## Specification

- **First sign-in registration** (`subspace-api/src/registration.ts`, `auth.ts`,
  `config.ts`, `api.ts`). A valid token for an unknown subject registers an
  active member with no initiative memberships, creating the organisation's row
  if it is missing, in one transaction serialised on that row, and audits it as
  `memberRegistered`. It is opt-in: `AUTO_REGISTER=entra` turns it on, and
  empty, unset or `none` is off. `workos` is refused at start-up until WorkOS
  gains an organisation allow-list, and development sign-in keeps registering its
  own members. A guest of the tenant is not registered unless
  `ENTRA_REGISTER_GUESTS=true`: the optional `acct` claim decides when present
  (1 is a guest), and otherwise an `idp` that is not the tenant's issuer. Profile
  claims fill in the profile only, and the e-mail comes only from `email`. A
  deactivated person stays refused; registration never reactivates anyone.
- **Administrators.** `ENTRA_ADMIN_ROLE` names an app role. While it is set, an
  Entra member's effective role is the one their token gives, recorded in
  `users.token_role` and audited (`memberRoleChanged`) only on a change;
  `users.role` keeps the role the membership command provisioned, so removing the
  setting withdraws its grants. The database refuses an agent a token role.
- **Usernames** (`subspace-api/src/usernames.ts`, migrations `usernames` and
  `retired_usernames`, `scripts/migrate.ts`, `scripts/username.ts`, `agents.ts`).
  One namespace per organisation for people and agents, compared in ASCII lower
  case (`lower(name COLLATE "C")`) whatever the database locale. An agent's
  username is its IRC nick, kept in step by a trigger. Allocation follows the
  issue's rule (first name, then successive letters of the surname, then a
  number), skipping Ergo's service names, `guest` and `guest-` names,
  `RESERVED_USERNAMES` and `RESERVED_USERNAMES_FILE`. A name that has been issued
  and then changed or cleared is recorded in `retired_usernames` by a trigger and
  never given to anyone else, unless an administrator releases it or hands it on
  with the username command, the only path the database allows. `bun run
  auth:username` sets a member's username or an agent's nick explicitly, audited,
  so the owner of an existing IRC account can be given it and an agent registered
  again can get its old nick back.
  `GET /me`, `Person` and `GET /users` report `username`.
- **IRC credentials and Ergo's hook** (`subspace-api/src/irc.ts`).
  `POST /api/v1/me/irc-credential` takes `{ "host", "port" }` and issues a
  stateless `sirc1.` credential, valid for ten minutes, only for an IRC server in
  `IRC_AUTH_SERVERS`, with its HMAC over the canonical host. On the main listener
  only. `POST /api/v1/irc/authenticate` answers Ergo's auth-script, authorised
  only by the hook token, always 200 once that is accepted, and accepts only a
  credential bound to a listed host. It is outside the OpenAPI document, the MCP
  tools, the bearer middleware and the agent listener.

## Decisions

- **Where the backfill runs.** Existing people receive usernames in
  `bun run db:migrate`, not in a SQL migration, because the rule needs the
  reserved names, which only the runtime configuration holds, and Unicode folding
  is then the same code as at registration. It runs after the schema migrations
  have committed, in batches of up to 100 people, each a short transaction that
  locks its organisation's row before touching a user and is retried on failure.
  It orders people by creation time and ID within each organisation and only
  fills in missing names, so it is deterministic and safe to rerun or to run from
  several instances. The SQL migration copies agents' nicks first.
- **One column and triggers** rather than a unique index over
  `coalesce(username, agent_irc_nick)`: the column is what clients read, and the
  triggers keep agents' usernames in step and record and refuse retired names at
  the database, including writes from an older instance.
- **A retired name may return to the record that held it**, so an agent's
  mistaken rename can be undone, but to no other record except through the
  username command's `--retired` or `--release`.
- **The credential does not carry its host.** The hook checks the signature
  against each listed host instead, which keeps the credential at 81 characters,
  under the 200 the contract allows for any host name.
- **Partial IRC configuration is refused at start-up**, including a credential
  secret without `IRC_AUTH_SERVERS` and the same value for both secrets. A hook
  left half-configured would otherwise answer 404 silently, and every IRC
  sign-in would fall back to Ergo's local accounts.
- **No audit events for hook decisions.** Ergo calls the hook for every SASL
  attempt, including local accounts' passwords, so their number has no bound.
  Decisions are logged at debug level with a reason and, for a correctly signed
  credential, the user ID; never the passphrase or the token.

## Trust

The hook token and the credential secret are the only things that let a caller
claim an IRC account. Anyone with the secret can mint a credential for any user
of the organisation for a listed server; anyone with the hook token can ask
whether a credential is good. Keep both random, distinct and in the deployment's
secret store.

Before turning the hook on for an IRC server that already has accounts, follow
the README's numbered cut-over: list them, reserve them all, give the ones people
own to their owners with the username command, then turn the hook on.

## Known limits

Recorded by the owner's decision for this internal system, rather than fixed:

- A database whose locale already let two names that differ only by a Turkish
  dotted and dotless i coexist will fail the new ASCII unique index, and needs
  reconciling by hand first.
- IPv6 literals in `IRC_AUTH_SERVERS` and in credential requests are compared as
  text, not normalised, and IPv4-mapped forms are not accepted in requests.
- Removing a port from `IRC_AUTH_SERVERS` while keeping the host leaves
  credentials already issued for that host usable until they expire, within ten
  minutes.
- A name claimed by direct SQL at the same moment as a rename can slip past the
  retirement trigger.
- Two requests carrying different token roles for one user at the same instant
  can record the wrong previous role in the audit row.

## Verification

- API: `bun run typecheck`, `bun run lint` and `bun test`; and, with
  `TEST_DATABASE_URL` naming a local PostgreSQL, the whole suite, including
  `tests/sign-in.test.ts`, `tests/usernames.test.ts`, `tests/irc.test.ts` and the
  migration tests. These run the real API process in-process against an isolated
  database with an invented Entra tenant whose keys only the test holds. The
  races hold the organisation row while the requests start, so they contend for
  the same name; the username race fails without the registration's lock.
- Not verified: a real Entra tenant's tokens, including the `idp` and `acct`
  claims of real guests, and a real Ergo calling the hook.

## Reviews

Round 1, on pull request 81: Claude Opus 5.5 clear with two P2; GPT-6.2 Sol not
clear with three P1. Every finding was fixed in a new commit:

- Credentials not bound to a server (both reviewers of the desktop change):
  bound to `IRC_AUTH_SERVERS`.
- Issued names reallocated after an agent's nick changed (Sol P1, Opus P2):
  retired names.
- Existing IRC accounts inherited by a namesake (Sol P1): reserved-names file,
  the username command and the numbered cut-over.
- An Entra admin grant outliving the setting (Sol P1): `token_role`.
- Registration on by default and open to guests (Opus P2): opt-in, guests refused.
- The backfill holding the users table while waiting (Sol P2): batched after the
  migrations.
- Locale-dependent name comparison (Sol P2): ASCII canonical form everywhere.
- The same value for both secrets (Sol P2): refused at start-up.
- `preferred_username` stored as the e-mail and secrets guidance (Opus P3): the
  e-mail comes only from `email`; the README shows how to generate the secrets.
- Missing tests for expired and untrusted tokens, oversized hook bodies and
  guests: added.
- Historical figures' names in fixtures (Sol P3): invented names.

Round 2, on head `a8d7131`: Claude Opus 5.5 clear with two P2; GPT-6.2 Sol not
clear. Fixed in new commits:

- `RESERVED_USERNAMES_FILE` not passed through Compose (both): passed through,
  with the override mount documented. Every other new setting was already
  passed through, and the root stack includes the same service.
- A retired name could never be given back (Opus P2): the username command acts
  on agents and can release or hand on a retired name; a trigger refuses any
  other path.
- `acct` not deciding guest status (Sol P1): it decides when present.
- Lock order in the username migrations (Sol P1): organisations before users.
- Name comparisons outside the canonical form, built-in names in the username
  command, and a historical name in a comment (Opus P3): fixed.
- Everything else is under Known limits.
