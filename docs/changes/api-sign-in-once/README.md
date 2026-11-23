# Sign in once: registration, username and IRC from one login

## Intent

A new person installs the desktop, signs in with their organisation's identity
provider, and can use the tracker and IRC straight away. Nobody runs a command,
registers an IRC account or sends a password. The API is the only identity
authority: Ergo never talks to an identity provider, but asks the API through its
auth-script hook.

This record covers the API's part, in three commits. The IRC image and the
desktop build on the contract in the API README's
[IRC sign-in](../../../subspace-api/README.md#irc-sign-in) section.

## Specification

- **First sign-in registration** (`subspace-api/src/registration.ts`, `auth.ts`,
  `config.ts`, `api.ts`). A valid token for an unknown subject registers an
  active member with no initiative memberships, creating the organisation's row
  if it is missing, in one transaction serialised on that row, and audits it as
  `memberRegistered`. `AUTO_REGISTER` lists the providers that do this; it
  defaults to `entra` when Entra is configured, and empty or `none` is off.
  `workos` is refused at start-up until WorkOS gains an organisation allow-list,
  and development sign-in keeps registering its own members. Profile claims
  (`name`, `email`, `preferred_username`, `given_name`, `family_name`) fill in the
  profile only. A deactivated person stays refused; registration never
  reactivates anyone. `ENTRA_ADMIN_ROLE` names an app role: when set, a token
  holding it is an administrator and one without it a member, applied on every
  request, written and audited (`memberRoleChanged`) only on a change, and never
  to an agent record.
- **Usernames** (`subspace-api/src/usernames.ts`, migration `usernames`,
  `scripts/migrate.ts`, `agents.ts`). `users.username` with one unique index on
  the organisation and the lower-case name: one case-insensitive namespace for
  people and agents. An agent's username is its IRC nick, written with it and
  kept in step by a trigger, so an older API instance that writes only the nick
  cannot split them. Allocation follows the issue's rule (first name, then
  successive letters of the surname, then a number), folded to lower-case ASCII,
  starting with a letter, at most 32 characters, skipping Ergo's service names,
  `guest`, `guest-` names and `RESERVED_USERNAMES`. It is serialised on the
  organisation row, the lock agent registration changes already take. An agent
  cannot register a person's username (409) or a reserved nick (400). `GET /me`,
  `Person` and `GET /users` report `username`.
- **IRC credentials and Ergo's hook** (`subspace-api/src/irc.ts`).
  `POST /api/v1/me/irc-credential` issues a stateless, HMAC-signed `sirc1.`
  credential valid for ten minutes, on the main listener only.
  `POST /api/v1/irc/authenticate` answers Ergo's auth-script, authorised only by
  the hook token, always 200 once that is accepted, outside the OpenAPI document,
  the MCP tools, the bearer middleware and the agent listener.

## Decisions

- **Where the backfill runs.** Existing people receive usernames in
  `bun run db:migrate`, after the SQL migrations and in the same transaction
  under the migration lock, not in a SQL migration. The rule needs the
  operator's `RESERVED_USERNAMES`, which only the runtime configuration holds,
  and Unicode folding is the same code as at registration rather than a second
  copy in SQL. It orders people by creation time and ID, so it is deterministic,
  and it only fills in missing usernames, so running it again changes nothing.
  The SQL migration copies agents' nicks first, so agents keep their names. A
  person still without a username, for example one added by an older instance
  during a rolling deployment, is given one on their next request.
- **One column and a trigger** rather than a unique index over
  `coalesce(username, agent_irc_nick)`: the column is what clients read, and the
  trigger gives the same database-level guarantee while letting a later explicit
  set be a plain write.
- **Partial IRC configuration is refused at start-up**: a hook token without an
  organisation, or either without the credential secret, would otherwise leave
  the hook silently answering 404 and every IRC sign-in falling back to Ergo's
  local accounts.
- **No audit events for hook decisions.** Ergo calls the hook for every SASL
  attempt, including local accounts' passwords, so their number has no bound.
  Decisions are logged at debug level with a reason and, for a correctly signed
  credential, the user ID; never the passphrase or the token.

## Trust

The hook token and the credential secret are the only things that let a caller
claim an IRC account. Anyone with the secret can mint a credential for any user
of the organisation; anyone with the hook token can ask whether a credential is
good. Keep both in the deployment's secret store.

Before turning the hook on for an IRC server that already has hand-made
accounts, make sure each account's owner has that username in the API, or a
different person with the same first name could be handed the existing account.

## Verification

- API: `bun run typecheck`, `bun run lint` and `bun test`; and, with
  `TEST_DATABASE_URL` naming a local PostgreSQL, `tests/sign-in.test.ts`,
  `tests/usernames.test.ts`, `tests/irc.test.ts` and the existing migration
  tests. These run the real API process in-process against an isolated database
  with a fictional Entra tenant whose keys only the test holds. The races hold
  the organisation row while the requests start, so they contend for the same
  name; the username race fails without the registration's lock.
- Not verified: a real Entra tenant's tokens and optional claims, and a real
  Ergo calling the hook.
