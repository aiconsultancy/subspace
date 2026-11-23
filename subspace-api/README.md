# Subspace API

Bun serves the v1 Subspace API. The HTTP contract is defined with Effect HttpApi
and Schema. Drizzle uses Effect's PostgreSQL driver for data access and migrations.

## Start the local API

From this directory, run:

```sh
docker compose up --build --wait
curl --fail http://127.0.0.1:3000/health
```

PostgreSQL and the API bind to host loopback. The API applies the committed SQL
migrations before serving requests. The two Compose volumes persist the database,
uploaded files and the development signing key.

Use `http://127.0.0.1:3000/api` as the desktop API base and select the development
identity provider. The first development sign-in to each organisation creates an
administrator and grants that user membership of the example initiative. Its
IRC endpoint defaults to `127.0.0.1:6667`. IRC accounts, channel setup and SASL
credentials belong to the IRC service, unless it signs people in through this API
as [IRC sign-in](#irc-sign-in) describes.

Mint a development bearer token with either command. Both use the same issuer,
key and membership records.

```sh
TOKEN=$(docker compose exec -T api bun run auth:dev dev@example.com local)
curl --fail -H "Authorization: Bearer $TOKEN" http://127.0.0.1:3000/api/v1/me/initiatives
```

```sh
TOKEN=$(curl --fail --silent http://127.0.0.1:3000/api/v1/auth/dev/token \
  -H 'Content-Type: application/json' -d '{"email":"dev@example.com","organisation":"local"}' \
  | jq -er .accessToken)
```

To add fictional demo data, run `docker compose exec -T api bun run seed:demo`.
It signs in as `developer@example.com` in organisation `local` through the
development endpoint and creates the fictional Fallow Reach rehearsal with
fleets, initiatives, projects, issues, decisions and attachments. Running it
again fills in missing fixtures.
`SEED_IRC_HOST`, `SEED_IRC_PLAIN_PORT` and `SEED_IRC_TLS_PORT` set the fleets'
IRC endpoints. From the repository root, `make stack-up` also adds IRC history and
primes the local GitHub webhook with a generated key for simulated deployment
and pull request events. The rehearsal timeline uses fixed offsets from Monday
at 00:00 UTC in the seed week. The root seed sets the creation timestamps
of new issues and comments and their creation events directly in the local
database. The four synthetic issue screenshots can be
regenerated with `python3 scripts/demo/generate-art.py` using Python 3.12 and
`rsvg-convert`.

The development endpoint accepts optional `email`, `name` and `organisation`
fields and returns `{accessToken,tokenType,expiresIn}`. Both `AUTH_PROVIDERS=dev`
and `ENABLE_DEV_AUTH=true` are required. The host process defaults to a loopback
listener. Local Compose opts in and publishes to loopback. Its
`DEV_CONTAINER_LOCAL_ONLY=true` setting allows the listener to bind inside the
container. Use that setting only with loopback port publishing and a trusted
container network. The default server provider outside Compose is WorkOS.

## External sign-in

Send the provider's **access token** in `Authorization: Bearer <token>`. The API
verifies the token and resolves an active local membership for its verified
provider, organisation and subject.

### First sign-in

A person who signs in with a configured identity provider is a member from their
first request: nobody runs a command first. When the token is valid and no record
exists for its subject, the API registers the person if their provider is listed
in `AUTO_REGISTER`. In one transaction it creates the organisation's record if it
is missing and an active member with no initiative memberships, and it audits the
registration as `memberRegistered`. Concurrent first requests from one person
create one member, and every one of them succeeds.

- `AUTO_REGISTER` is a comma-separated list of providers. It defaults to `entra`
  when Entra is configured, because the tenant, audience and client are already
  pinned. An empty value or `none` turns registration off, and an unknown person
  then receives 403. `workos` is refused at start-up until WorkOS registration
  gains an organisation allow-list. Development sign-in registers its own members,
  as before, and is not listed.
- The token's claims fill in the profile, never the identity. The display name is
  `name`, else the given and family names, else the e-mail. The e-mail is `email`,
  else `preferred_username`, when it has the form of an address. Entra puts `name`
  and `preferred_username` in v2 access tokens when the client asks for the
  `profile` scope. Add `email`, `given_name` and `family_name` as optional claims on
  the **access token** in the API's app registration, so that usernames come from
  the given and family names.
- Registration admits whoever the tenant lets obtain an access token for the API,
  guests included. To limit it to chosen people or groups, turn on _Assignment
  required_ for the enterprise applications in Entra and assign them.
- A token for another tenant, audience or client is refused with 401 and creates
  nothing. A deactivated person stays refused with 403: registration never
  reactivates anyone, and only the membership command below does.

`ENTRA_ADMIN_ROLE` names an app role defined on the API's app registration. When it
is set, an Entra token whose `roles` claim holds that value makes its holder an
administrator, and a token without it makes them a member. The API checks this on
every request, writes the role only when it changes and audits each change as
`memberRoleChanged`. The app role is then the only source of the administrator
role: it overrides `--role` from the membership command at the person's next
request. When `ENTRA_ADMIN_ROLE` is unset, roles change only through the
membership command. An agent record is never an administrator.

### The membership command

To grant access before a person's first request, to provision a member when
registration is off, or to revoke access, run the membership command with the
verified organisation and subject from a trusted IdP administrator:

```sh
bun run auth:member workos org_example user_example --name "Example Operator" --role admin
```

For WorkOS, use the organisation ID in `org_id` and the user ID in `sub`.
`WORKOS_CLIENT_ID` must be `client_` followed by letters and digits, the same
rule the desktop applies to `SUBSPACE_WORKOS_CLIENT_ID`. Verification uses the configured client's session JWKS, the exact issuer and
a five-second clock tolerance. Set `WORKOS_ISSUER` for a custom auth domain and
`WORKOS_AUDIENCE` if the deployment configures a token audience. An
optional `WORKOS_API_KEY` lets this provisioning command fetch a missing profile
name or email. For Entra, use the tenant UUID in `tid` and user object UUID in
`oid`. The access token must use the tenant's v2 issuer, identify the configured client
in `azp`, contain the delegated API scope and use the API application's client
UUID as its audience. Use `--deactivate` with the same organisation and subject
to revoke API access.

### Usernames

Every person has a `username`, which is also their IRC account name. People and
agents share one namespace in each organisation, compared without regard to case:
an agent's username is its IRC nick (`ircNick`), and the database keeps the two in
step on every write. A unique index on the organisation and the lower-case
username guarantees that no two records share a name.

The API allocates a username when it registers a person, and the membership
command and development sign-in do the same for the people they add.
Development sign-in without a name takes the username from the e-mail.

1. The first name is `given_name`. Without one, it is the first word of `name`
   (`Quarrie, Tamsin` names the surname first), then the e-mail local part read as
   `first.last`, then `user`. The surname is `family_name`, else the rest of `name`
   or of the local part.
2. Names are folded to lower-case ASCII: diacritics are removed, letters such as
   `ß` and `ø` are spelt out, anything outside `a` to `z` and `0` to `9` is dropped,
   leading digits are dropped, and the result has at most 32 characters. Ergo
   accepts every such name as an account.
3. The first free name wins: the first name, then the first name with successive
   letters of the surname, then the first name with a number from 2. Tamsin Quarrie
   is `tamsin`, or `tamsinq` when `tamsin` is taken, then `tamsinqu` and so on, then
   `tamsin2`.
4. Reserved names are skipped: Ergo's services (`nickserv`, `chanserv`,
   `operserv`, `hostserv`, `histserv`, `botserv`, `memoserv` and `global`), `guest`
   and any name starting `guest-`, and the operator's names in
   `RESERVED_USERNAMES`.

Allocations in an organisation are serialised with each other and with agent
registration changes, so people who register at the same moment with the same
first name receive different usernames. An agent cannot register an IRC nick that
is a person's username (409) or a reserved name (400). A username never changes by
itself; the API does not yet offer a way to change one.

`bun run db:migrate`, which the image runs at start-up, gives existing people
usernames by the same rule, oldest first, after applying the migrations and in the
same transaction. It runs there rather than in a SQL migration because it needs
`RESERVED_USERNAMES`, and it only fills in missing usernames, so running it again
changes nothing. A person added without one, such as by an older API instance
during a rolling deployment, is given one on their next request, audited as
`usernameAllocated`.

`GET /api/v1/me`, every `Person` (assignees, authors and other credited people)
and `GET /api/v1/users` report `username`. It is null only for an agent without
an IRC nick.

Before turning on IRC sign-in through the API for a server that already has
hand-made accounts, make sure each such account's owner has that username here.
Otherwise a different person with the same first name could be handed the existing
account.

Local administrators can manage fleets, fleet assignments and initiative channel
configuration. Initiative membership is explicit. Users see active, explicitly
assigned initiatives in their desktop picker. All tenant resource queries are
scoped to the local organisation, and composite foreign keys prevent cross-tenant
links.

### Configuration

| Variable                                                              | Meaning                                                                                                         |
| --------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `GITHUB_WEBHOOK_SECRET`                                               | Optional signature secret for pull request and deployment status webhooks.                                      |
| `GITHUB_ORGANISATION_ID`                                              | Internal organisation UUID. Required for GitHub App or webhook credentials.                                     |
| `GITHUB_APP_ID`, `GITHUB_APP_INSTALLATION_ID`                         | Optional GitHub App used to reconcile mapped pull requests.                                                     |
| `GITHUB_APP_PRIVATE_KEY_FILE`, `GITHUB_APP_PRIVATE_KEY`               | Private RSA PEM mounted as a file or supplied in the environment. Configure one key source.                     |
| `GITHUB_POLL_INTERVAL_SECONDS`                                        | Poll interval, defaulting to 120 seconds.                                                                       |
| `DATABASE_POOL_SIZE`                                                  | PostgreSQL connections per API process. Defaults to 10, with one reserved for updates.                          |
| `SSE_MAX_STREAMS_PER_USER`, `SSE_MAX_STREAMS`                         | Concurrent update streams per user and per process. Default to 16 and 256.                                      |
| `DATABASE_URL`                                                        | PostgreSQL connection URL. Required outside Compose.                                                            |
| `HOST`, `PORT`                                                        | Server listener, defaulting to `127.0.0.1:3000` outside the container.                                          |
| `API_PORT`, `POSTGRES_PORT`, `POSTGRES_PASSWORD`                      | Compose host ports and local PostgreSQL password.                                                               |
| `AUTH_PROVIDERS`                                                      | Comma-separated `workos`, `entra` or `dev`.                                                                     |
| `ENABLE_DEV_AUTH`                                                     | Explicit development sign-in opt-in. Local Compose sets this to `true`.                                         |
| `WORKOS_CLIENT_ID`, `WORKOS_ISSUER`, `WORKOS_JWKS_URL`                | AuthKit client, issuer (default `https://api.workos.com/`) and optional JWKS override.                          |
| `WORKOS_AUDIENCE`                                                     | Optional exact audience for AuthKit deployments that set an audience claim.                                     |
| `WORKOS_API_KEY`                                                      | Optional profile lookup credential for the membership CLI.                                                      |
| `ENTRA_TENANT_ID`, `ENTRA_CLIENT_ID`, `ENTRA_AUDIENCE`, `ENTRA_SCOPE` | Entra tenant, authorised client, API audience and delegated scope (default `API.Access`).                       |
| `AUTO_REGISTER`                                                       | Providers that register a person at [first sign-in](#first-sign-in). Empty or `none` is off.                    |
| `ENTRA_ADMIN_ROLE`                                                    | Optional Entra app role whose holders are administrators, checked on every request.                             |
| `RESERVED_USERNAMES`                                                  | Comma-separated operator names that no person or agent may take as a [username](#usernames).                    |
| `AGENT_PORT`, `AGENT_HOST`, `AGENT_ORGANISATION`                      | Optional [agent listener](#the-agent-listener): port (off when unset), address and `<provider>:<organisation>`. |
| `DEV_KEY_PATH`                                                        | Private development signing key file, defaulting to `.data/dev-signing-key`.                                    |
| `DEV_CHANNELS`, `DEV_IRC_HOST`, `DEV_IRC_PORT`                        | Channel text and IRC endpoint for newly created dev organisations.                                              |
| `STORAGE_DRIVER`, `STORAGE_PATH`                                      | Defaults to `local` and `.data/blobs`. Use `s3` for a private S3-compatible bucket.                             |
| `S3_BUCKET`, `S3_ENDPOINT`, `S3_REGION`                               | S3 bucket, optional custom endpoint and region.                                                                 |
| `S3_ACCESS_KEY_ID`, `S3_SECRET_ACCESS_KEY`, `S3_SESSION_TOKEN`        | Credentials and optional session token supplied to Bun's S3 client.                                             |
| `IRC_CREDENTIAL_SECRET`, `IRC_CREDENTIAL_SECRET_FILE`                 | Secret of at least 32 bytes that signs [IRC credentials](#irc-sign-in). Set one source; unset is off.           |
| `IRC_AUTH_HOOK_TOKEN`, `IRC_AUTH_HOOK_TOKEN_FILE`                     | Bearer token of at least 32 printable bytes for Ergo's auth-script. Set one source.                             |
| `IRC_AUTH_ORGANISATION`                                               | `<provider>:<organisation>` whose people the IRC server signs in, such as `entra:<tenant UUID>`.                |

The image defaults to `NODE_ENV=production`, which forbids development sign-in.
Local Compose defaults to `NODE_ENV=development`. Production deployments use TLS
for clients and the backing database and take credentials from runtime configuration.

Update streams return 429 at the configured limits and 503 while the listener
reconnects. Clients can retry with backoff and refresh their issue data over REST.

## IRC sign-in

The API can be the identity authority for an Ergo IRC server, so that a person
who signed in to the API also signs in to IRC with nothing set up by hand. The
desktop asks the API for a short-lived credential and gives it to Ergo with SASL
PLAIN. Ergo's auth-script asks the API whether it is good, and Ergo creates the
account on the first success. Ergo never talks to an identity provider, so this
works for every provider the API accepts. The two endpoints below are the
contract; another implementation of this API must match them exactly.

Generate each secret with, for example, `openssl rand -base64 48`, and keep both
in the deployment's secret store or a mounted file.

### `POST /api/v1/me/irc-credential`

Any signed-in, active user, human or agent, with their bearer access token on the
main listener. The agent listener does not serve it, because anyone who reaches
that listener can act as any agent. The request has no body.

```json
{ "account": "tamsin", "password": "sirc1.<opaque>", "expiresAt": "2026-11-23T21:10:00Z" }
```

- `account` is the user's [username](#usernames). For an agent it is its IRC nick.
- `password` is opaque to clients, at most 200 characters, valid for ten minutes
  and reusable until it expires. Clients fetch a new one at each connection and
  keep it in memory only.
- `expiresAt` is the expiry in UTC to the second.
- The response carries `Cache-Control: private, no-store`.
- The usual 401 and 403 apply to a missing, invalid or expired token and a
  deactivated member. The route answers 404 when `IRC_CREDENTIAL_SECRET` is not
  set, and 409 for an agent without an IRC nick.

A credential is `sirc1.` followed by the unpadded base64url encoding of 56 bytes:

1. the user's UUID as 16 bytes, in the order its hexadecimal digits are written;
2. the expiry as an unsigned 64-bit big-endian count of seconds since the Unix
   epoch;
3. an HMAC-SHA256, keyed by the UTF-8 bytes of `IRC_CREDENTIAL_SECRET`, over the
   ASCII text `sirc1.` followed by the 24 bytes of items 1 and 2.

It is stateless: nothing is stored. A credential is good while the current time
is before its expiry and the expiry is no more than 660 seconds ahead. The
signature is compared in constant time.

### `POST /api/v1/irc/authenticate`

Called only by Ergo's auth-script. It is not part of the OpenAPI document or the
MCP tools, it is not behind the bearer middleware, and the agent listener does
not serve it.

- The route answers 404 unless `IRC_AUTH_HOOK_TOKEN`, `IRC_AUTH_ORGANISATION` and
  `IRC_CREDENTIAL_SECRET` are all set. The API refuses to start with only one of
  the first two, or with them but without the credential secret.
- It is authorised only by `Authorization: Bearer <IRC_AUTH_HOOK_TOKEN>`, compared
  in constant time. A missing or wrong token receives 401 with a problem document
  and `WWW-Authenticate: Bearer`, before the body is read.
- The body is Ergo's auth-script input as it is:
  `{ "accountName"?, "passphrase"?, "certfp"?, "peerCerts"?, "ip"? }`. Other
  properties, such as `oauth2` and `cookies`, are ignored, and so is a body over
  256 KiB.
- Once the token is accepted, the answer is always 200 with
  `{ "success": true, "accountName": "tamsin" }` or `{ "success": false }`, never a
  problem document, including for a malformed body or a database failure.

The rules, in order:

1. A `passphrase` that starts with `sirc1.` succeeds only if its signature is
   this server's, it has not expired, its user exists, is active, belongs to the
   organisation in `IRC_AUTH_ORGANISATION` and has a username, and the
   `accountName`, ignoring an `@` and anything after it and compared without
   regard to case, is that username. The answer names the canonical username,
   which for an agent is its nick as registered.
2. A `certfp` with no passphrase answers `success: false`. It is reserved for
   agent key credentials.
3. Anything else answers `success: false`, so Ergo falls back to its own
   accounts: the break-glass operator and any hand-made accounts.

The API never logs a passphrase or the hook token. It logs each decision at debug
level with its reason and, for a correctly signed credential, the user ID. It
does not write audit events for these decisions, because Ergo calls the hook for
every SASL attempt, so their number has no bound.

## GitHub App and webhooks

Use a GitHub App with pull request read access on the repositories this API
should observe. Configure the App and installation IDs and supply its RSA
private key, preferably as a read-only file readable by the API user. Each
API instance periodically obtains an installation token and reconciles recent
pull requests with a durable checkpoint per mapped repository. The integration
status endpoint at `GET /api/v1/integrations/github/status` reports poll times,
counts and errors for the caller's organisation. [GitHub App authentication](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-a-json-web-token-jwt-for-a-github-app)

Set `GITHUB_WEBHOOK_SECRET` and configure GitHub to send `pull_request` and
`deployment_status` events to `POST /api/v1/integrations/github/webhook`. Set
`GITHUB_ORGANISATION_ID` to the internal UUID of the one organisation served by
this integration. Signed events and polling are scoped to that organisation.
Its administrators map repositories with `PUT /api/v1/projects/{reference}/repositories`
using the current project version and an array of `owner/name` strings. The endpoint also offers
single-repository add and remove operations. With an App configured, repository
writes check the installation's accessible repository list, and signed deliveries
must carry its installation ID. With webhook-only configuration, the trusted
sender supplies events for the single configured organisation. Unconfigured
stacks still support project repository links and reverse lookup.

Observed pull requests link issues in mapped projects. Closing references in
titles and bodies on newly observed merges move eligible issues to the first
completed workflow state. Mentions, historical merges and archived or terminal
issues retain their existing state. Issue link and state changes refresh the
desktop through `GET /api/v1/updates?project=KEY`. The desktop separately polls
its favourite repositories to send merge and deployment notices to captains on
IRC. The API records signed deployment statuses, which project members can read
at `GET /api/v1/projects/{reference}/deployment-statuses`. GitHub's deployment
status events contain the environment, commit and state. [GitHub deployment statuses](https://docs.github.com/en/rest/deployments/statuses)

The API's polling App is a service identity. The fleet installer provisions
individual agent Apps for Git and GitHub actions.

## Fleets and ships

Authenticated members can read `GET /api/v1/fleets` and look up a fleet by
slug or ID. Creating or editing a fleet requires a human organisation administrator.
Creating an initiative also requires a human organisation administrator.
Human members can create ships under `/api/v1/fleets/{fleetId}/ships` and rename them
with `PATCH /api/v1/ships/{shipId}`. The fleet reports its agents in rank and
name order, and its ships in name order. The deprecated leadership fields
`rearAdmiralName` and `captainNames` come from linked agents.

A fleet records its standard IRC channels: `starbaseChannel` (people, the
rear admiral and the captains) and `commandChannel` (the rear admiral and its
captains), each optional and set on create or with `PATCH`. A ship records its
captain's ship channel in `channel`, set when the ship is created or renamed;
a rename that leaves `channel` out keeps it, and `null` clears it. Because a
ship's channel decides every member's autojoin, as the fleet's channels do,
only a human organisation administrator may set or change it; a member who
creates a ship with a channel, or renames one with a different channel,
receives 403. Channel
names start with `#` and hold no whitespace, commas, colons or control
characters, so the desktop joins each as one channel. The
fleet's `agents` repeat each linked ship's channel as `shipChannel`. An
initiative's `channels` are its own channels beyond its fleet's standard ones.

A fleet also records `fleetApiUrl`, the address at which a desktop reaches
the fleet's Stargate Fleet API to show its live state. The fleet installer
sends it when it registers the fleet. It is an absolute `https` URL on any
host, or `http` only on `localhost`, `127.0.0.1` or `[::1]`, such as the local
end of an SSH tunnel. It carries no credentials, query or fragment and is at
most 2,048 characters. The API stores it with a lower-case scheme and host and
no trailing slash. It is optional on create; an edit that leaves it out keeps
it, and `null` clears it. Like the fleet's other fields, only a human
organisation administrator can set it, never an agent, because the desktop
sends the fleet's token to that address. The token itself is never stored in or
returned by this API: the desktop app asks for the fleet's token once and keeps
it on the user's own machine. Fleet responses, including the fleet embedded in
initiative responses, report it.

Each agent may record `ircNick`, its nick on the fleet's IRC server, which
fleet IRC servers tie to the account of the same name. Desktop clients mark an
agent's rank in IRC only by this nick, never by its name. A nick starts with a
letter and holds letters, digits, `-` and `_`, at most 64 characters, and is
unique within the organisation, ignoring case. It is also the agent's
[username](#usernames), so it cannot be a person's username or a reserved name.
An edit that leaves `ircNick` out keeps it; `null` clears it. The fleet's
`agents` report it.

Agent registration and editing accept `fleetId` and `shipId`. An ID takes
precedence over its text field. Legacy fleet names resolve within the caller's
organisation. A ship name creates a ship only within a matched fleet. Captains
need a ship and rear admirals cannot have one.

The migration preserves legacy agent text and leaves unmatched fleets
unlinked. It deduplicates ship names within each matched fleet. The upgrade
retains old configured leadership values in the database for reference. New
roster responses use registered agents. Apply the
migration and deploy the API before clients submit IDs. Update fleet installers
with this release so that they send only the editable fleet fields. The MCP tools
`subspace_list_fleets`, `subspace_get_fleet`, `subspace_create_fleet`,
`subspace_update_fleet`, `subspace_list_ships`, `subspace_create_ship` and
`subspace_rename_ship` follow the same access and validation rules.

## MCP and agents

Configure an MCP client with the Streamable HTTP URL
`http://127.0.0.1:3000/mcp` for local development, or the HTTPS equivalent for a
deployed service. Supply `Authorization: Bearer <accessToken>` on each request.
Stargate's Claude backend accepts an HTTP entry in `backend.mcpServers` and an
operator-controlled `headersHelper` that supplies the bearer header. Configure
OpenCode's remote MCP connection or the equivalent connection for your harness
using the same URL and header. Keep the bearer in the harness's credential store
and refresh it before expiry. `tools/list` gives the current input schemas.
Issue, comment, checklist, planning, decision, review and document mutations
share the REST services and their version and authorisation checks.
[Streamable HTTP transport](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)

Human organisation administrators register and deactivate API agent identities under
`/api/v1/agents`. Register an agent with `name`, `rank`, `fleet`, `defaultModel`,
`serviceTokenClientId`, `ship` for captains and, optionally, `ircNick`. The ranks are `rearAdmiral`,
`captain` and `crew`. The `serviceTokenClientId` field carries the verified
provider subject for the agent in that organisation. Use the WorkOS `sub`,
the Entra `oid` or the email used by local dev sign-in. Authenticate as that
provider account to use the API. A bearer for a deactivated identity is refused
on subsequent requests. Humans can read organisation audit events. Agents can
read project audit events. Captains and rear admirals can attach crew name and
model to issue and comment writes. The authenticated agent remains the actor.

### The agent listener

A registered agent can also use the API with no identity provider and no
token, on a second, agent-only listener. Set `AGENT_PORT` to turn it on;
`AGENT_HOST` defaults to `HOST`, and `AGENT_ORGANISATION` names the one
organisation it serves as `<provider>:<organisation>`, defaulting to
`dev:local` when development sign-in is on; its provider must be one of
`AUTH_PROVIDERS`. It keeps the main listener's
rules: loopback, or `DEV_CONTAINER_LOCAL_ONLY=true` with local-only port
publishing, and never with `NODE_ENV=production`. The main listener is
unchanged.

On it, each request names its agent in `X-Subspace-Agent: <agent name or IRC
nick>`, matched without regard to case. The API resolves that to an active,
registered agent record of the configured organisation and acts as it:
`kind: "agent"`, its rank, fleet and ship, with every rule that applies to
agents, including the fleet limits on assignments, Ready for agents and moves.
It never resolves a person, so it never grants the administrator role, which
the database already forbids an agent record. A missing header, an unknown
name, a name that matches more than one active agent, or a person's name or
subject receives 401; a name that matches only deactivated agents receives
403, and a deactivated record sharing the name or nick of an active agent
does not hide it; any bearer token is ignored. Before any route runs, the listener also refuses with 403 any request
that carries an `Origin` header and any `Host` other than `127.0.0.1`,
`localhost` or `[::1]` (with an optional port), so a browser page that
rebinds its domain to the listener's address cannot act as an agent. Docker's
loopback port publishing passes the `Host` through unchanged. Captains and rear admirals
relay crew work with `relayedCrew` as on the main listener.

The listener serves the same routes and MCP endpoint (`/mcp`) except
development sign-in, the signed GitHub webhook, `/openapi.json` and the
administrator routes: creating and editing fleets, creating initiatives,
their channel configuration and fleet assignment, registering, editing,
deactivating and reactivating agents, and the GitHub integration status.
The listener's router never registers those, so they answer 404 however the path is spelt, and `tools/list` omits
`subspace_create_fleet` and `subspace_update_fleet`. A registered agent also
counts as a member of every initiative that belongs to its fleet, so
`/me/initiatives` and `subspace_list_initiatives` list its fleet's work on
either listener; other reads and writes need no initiative membership.

This trusts the listener's network as development sign-in does, minus
administrator access: anyone who can reach the agent port can act as any
registered agent of that organisation, and agents can act as each other.
Expose it only to the agents' own users: the fleet installer's lockdown admits
only root, the operator and the officers' Linux users to it.

Issue creators, assignees and commenters follow issues automatically unless
they explicitly unsubscribe. A member can change their subscription under
`/api/v1/issues/{reference}/subscription` and manage their own inbox at
`/api/v1/notifications`. The API sends in-app notices for comments, linked
Markdown mentions and state changes.

## Ready for agents

Every full issue shape includes `readyForAgents`, a boolean that defaults to
`false`: list and group items, issue detail and its `children`, relations,
cycle results and trackers, move results and MCP issue results. The embedded
`parent` and `duplicateOf` references in issue detail do not carry it; read
the referenced issue for its flag.

The flag marks an unassigned, open issue as available for a fleet's captains
to take. Anyone who may edit an issue can set it on create with
`POST /api/v1/issues` or on edit with `PATCH /api/v1/issues/{reference}`.
An agent registered to a fleet can set it only on issues its fleet may
dispatch. See [Fleet ownership](#fleet-ownership).

- A write that would leave the issue with an assignee and asks for
  `readyForAgents: true` fails with 400. An edit that clears the assignee and
  sets the flag in the same request is allowed.
- Any write that leaves the issue with an assignee sets `readyForAgents` to
  `false` in the same write and version. Accepting an offer is therefore one
  versioned assignment, and a second writer with the same version receives 409.
- Only open issues that are not trackers can be flagged. Asking for
  `readyForAgents: true` on a write that leaves the issue a tracker or in a
  completed, canceled or duplicate state fails with 400.
- A write that makes the issue a tracker, moves it to a terminal state or
  archives it clears the flag in the same write and version. This includes
  marking it a duplicate of another issue, a pull request merge that completes
  it and deleting its workflow state with a terminal replacement.
- Removing the assignee, reopening or unarchiving does not set the flag again.
- The issue's activity lists `readyForAgents` among the changed fields
  whenever the flag changes, including each time the server clears it. An
  archive that clears it records `fields: ["readyForAgents"]` on the
  `issue.archived` event.

A partial index on the organisation covers issues that are flagged,
unassigned and not archived, for the dispatcher's poll. A database trigger also
clears the flag whenever a write leaves an assignee, so an older API instance
during a rolling deployment cannot leave an assigned issue flagged. That
clearing has no activity entry.

`GET /api/v1/issues` and `GET /api/v1/issues/groups` take
`readyForAgents=true` or `readyForAgents=false`, validated like the other
boolean filters. Ask for unassigned issues with the existing `assignee=none`,
so a poller reads `readyForAgents=true&assignee=none`. The `assignee` filter
also accepts an agent's user ID and combines with `updatedSince`. The MCP
tools `subspace_create_issue`, `subspace_update_issue` and
`subspace_list_issues` accept the same field and filter, and every MCP issue
result carries the field.

## Fleet ownership

A project belongs to its initiative's fleet, or else to the fleet of that
initiative's nearest ancestor with one. A project without an initiative, or
with no fleet anywhere on its initiative path, belongs to no fleet.

An agent registered to a fleet, shown by `kind: "agent"` and a non-null
`fleetId` on `GET /api/v1/me`, may dispatch issues only in its own fleet's
projects and in projects that belong to no fleet. Dispatching means sending a
non-null `assigneeId`, or `readyForAgents` with either value, to
`POST /api/v1/issues` or `PATCH /api/v1/issues/{reference}`. When the issue's
project belongs to another fleet, the API refuses the write with 403 and the
usual problem document, and changes nothing: no issue is created, and the
issue's fields, version and activity stay as they were. On edit, the archive,
version and empty-change checks come first, so a stale version still
receives 409.

Moves follow the same ownership. When such an agent calls
`POST /api/v1/issues/{reference}/move`, the API refuses the move with 403 if
the source or the destination project belongs to another fleet, and moves
nothing. Moves between the agent's own fleet's projects and projects without
a fleet are allowed. The checks on the projects, the issue version and the
moved issues come first, and `POST /api/v1/issues/{reference}/move-preview`
does not apply the ownership check.

Because initiatives inherit their fleet in the same way, reparenting follows
the same ownership. An initiative belongs to its own fleet, or else to the
fleet of its nearest ancestor with one. An initiative's owner can be an agent,
and owners may change `parentId` with `PATCH /api/v1/initiatives/{id}`. When
such an agent changes `parentId`, the API refuses the change with 403 if the
initiative, or the new parent, belongs to another fleet, and changes nothing.
This stops an agent taking another fleet's branch, and the projects under it,
out of that fleet, and stops it placing a branch under another fleet. Moves to
and from the agent's own fleet's initiatives and initiatives without a fleet,
including making the initiative a root, are allowed. The permission, version,
archive and hierarchy checks come first. Setting or clearing an initiative's
fleet with `PUT /api/v1/initiatives/{id}/fleet` needs an organisation
administrator, which an agent never is, and no MCP tool changes an
initiative's parent or fleet.

- Humans and agents without a fleet are not restricted.
- Clearing the assignee with `assigneeId: null` is allowed, as are edits that
  send neither field. The server clearing the flag as a side effect of
  another change, such as a terminal state, is not a dispatch.
- Comments are not restricted. They change no ownership or dispatch state,
  and fleets legitimately discuss each other's work.
- The MCP tools `subspace_create_issue`, `subspace_update_issue` and
  `subspace_move_issue` apply the same rules and return the same problems as
  tool errors.

The check runs inside the write's transaction. It reads the agent's
registration, the project and each initiative on the path up to the first one
with a fleet, holding a share lock on each row until the write commits. A
move checks its source and destination in project ID order, the order in
which it already locks the two project rows. A reparent checks the initiative
and its new parent in initiative ID order, after taking the organisation's
initiative write lock. A concurrent project move, initiative reparenting,
fleet assignment or agent re-registration therefore either commits first and
is enforced, or waits until the write has committed.
A dispatcher that read an issue before its project moved to another fleet
receives 403 even though the issue version is unchanged.

## Planning and stakeholders

Initiative responses include `stakeholders`, a list of names, roles and Slack
or Teams handles. Use `/api/v1/initiatives/{id}/stakeholders` to read the list
or create an entry. Read, patch or delete a single entry by adding its UUID.
The human initiative owner or an organisation administrator can make changes
on an unarchived branch. Each entry needs at least one contact handle.

Project cycle settings define the length, start day, number of upcoming cycles
and carry-over behaviour. Call `POST /api/v1/projects/{reference}/cycles/reconcile`
to advance an enabled schedule, for example from an operator's scheduler.
Closed cycles retain their issue outcomes. Review requirements evaluate approved
reviews for an exact document revision or external subject and revision.
Decision and document revisions are available through the versioned REST routes.

## Decisions

`GET /api/v1/decisions` sorts by creation time and ID, newest first. It
filters by `issue`, `project`, `status`, `madeByType`, and the exact
lower-case `tag`. Use `q` to search the title, decision, context, why,
implications and alternatives without matching case.

List items and detail responses return `recordedBy` and `madeBy` with
`userId`, `displayName`, `available` and `kind`. The recorder is the
authenticated member who recorded the decision. The maker is the attributed
human or registered agent. `madeBy` is null when the maker has no registered
account, such as relayed crew. Only captains and rear admirals may send
explicit crew provenance. A deactivated member stays named with
`available` set to false. Revision snapshots retain the attribution saved
with that revision.

`GET /api/v1/decisions/counts` counts the decisions the list would return in
each status, as `{"proposed":1,"accepted":4,"rejected":1,"superseded":2,"total":8}`.
Every key is always present and `total` is the sum of the other four. It takes
the list's filters except `status`, with the same validation, access rules and
errors, and ignores `status` and paging parameters as the list ignores an
unknown parameter. The MCP tools `subspace_list_decisions` and
`subspace_count_decisions` return the same items and counts.

## Contract and verification

See [OpenAPI](openapi.json) for versioned routes, request shapes, status codes and
responses. The running server serves its enabled contract at `/openapi.json`.
Fleets have a flat IRC configuration for installer writes and an additive
`irc` object for desktop reads. Assign and unassign a fleet with
`PUT /api/v1/initiatives/{id}/fleet`, supplying a fleet UUID or null in
`fleetId`. Initiative list, detail and member responses include the assigned fleet.

```sh
bun install --frozen-lockfile
bun run typecheck
bun run lint
bun test
TEST_API_URL=http://127.0.0.1:3000 TEST_DATABASE_URL=postgresql://subspace:subspace-local@127.0.0.1:5432/subspace bun test
bun run openapi
```

The repository's pre-push hook runs type checking, lint and `bun test` on API
code changes. Install the API dependencies before pushing code.

The HTTP tests require a clean loopback Compose stack and create synthetic
organisations. The agent listener's HTTP tests also need `AGENT_PORT` published by
that stack, with `TEST_AGENT_API_URL` set to it and `TEST_AGENT_ORGANISATION`
to the development organisation slug its `AGENT_ORGANISATION` names. The migration test creates an isolated database on the local PostgreSQL server named in `TEST_DATABASE_URL`.
The sign-in, username and IRC tests need only `TEST_DATABASE_URL`: each creates
an isolated database there and runs the API in-process, trusting a fictional
Entra tenant whose keys only the test holds.
The fleet ownership race test holds a row lock in the API's own database while
an agent writes, so when both variables are set, `TEST_DATABASE_URL` must name
the database that `TEST_API_URL` serves, as in the example above. For signed pull request and deployment tests, sign in first as
`admin@example.com` to a dedicated dev organisation. Start the API with that
organisation's internal UUID in `GITHUB_ORGANISATION_ID` and a local
`GITHUB_WEBHOOK_SECRET`. Set `TEST_GITHUB_ORGANISATION` to its dev sign-in slug
and `TEST_GITHUB_WEBHOOK_SECRET` to the same secret when running the suite.
The SSE load test defaults to the Compose limits. For a smaller fixture set
`DATABASE_POOL_SIZE=4`, `SSE_MAX_STREAMS_PER_USER=6` and `SSE_MAX_STREAMS=8`
in Compose and pass the matching `TEST_` variables to the test runner.

For host development, export `DATABASE_URL` for the published PostgreSQL port,
set `AUTH_PROVIDERS=dev` and `ENABLE_DEV_AUTH=true`, then run
`bun run db:migrate` followed by `bun run dev`. After a schema change, use
`bun run db:generate` and commit the SQL and snapshot.

Drizzle ORM and Drizzle Kit are pinned together at `1.0.0-rc.5` for the Effect
PostgreSQL adapter and its migration format. Upgrade both together once a
stable release supports the adapter. Before switching, verify the existing
migration journal and snapshots, apply the full history to a fresh database,
upgrade a database created with the previous pin, and rerun the Compose suite.

## Mapping an earlier database

The TypeScript service owns a new schema. A historical import can map the
earlier database in this order.

1. Establish organisations and map verified identity subjects to users and
   agent registrations. Preserve UUIDs where possible and retain a key map for
   any changed identifiers.
2. Import fleets, initiatives, memberships, projects, workflow states, labels,
   milestones and cycles before issues. Preserve issue identifiers, aliases,
   counters and versions, and defer parent and relation links until their
   targets exist.
3. Map comments and threads, document and decision revisions, review targets,
   subscriptions and notifications. Map historical actor IDs, timestamps and
   audit data with the identity map. Migrate media metadata and file bytes
   together and verify each digest.
4. Import repository mappings, observed pull requests, link outcomes and cycle
   results. Treat imported merges as historical observations. Validate
   tenant-scoped foreign keys, counts, references and representative API reads
   before selecting the imported database.

Suppress live notifications during the import and use an isolated target for
verification. File storage supports local disk and private S3-compatible storage.
