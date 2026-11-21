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
credentials belong to the IRC service.

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
verifies the token and resolves an active local membership. To grant access
before the user's first request, provision their verified organisation and
subject from a trusted IdP administrator:

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

Local administrators can manage fleets, fleet assignments and initiative channel
configuration. Initiative membership is explicit. Users see active, explicitly
assigned initiatives in their desktop picker. All tenant resource queries are
scoped to the local organisation, and composite foreign keys prevent cross-tenant
links.

### Configuration

| Variable                                                              | Meaning                                                                                     |
| --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `GITHUB_WEBHOOK_SECRET`                                               | Optional signature secret for pull request and deployment status webhooks.                  |
| `GITHUB_ORGANISATION_ID`                                              | Internal organisation UUID. Required for GitHub App or webhook credentials.                 |
| `GITHUB_APP_ID`, `GITHUB_APP_INSTALLATION_ID`                         | Optional GitHub App used to reconcile mapped pull requests.                                 |
| `GITHUB_APP_PRIVATE_KEY_FILE`, `GITHUB_APP_PRIVATE_KEY`               | Private RSA PEM mounted as a file or supplied in the environment. Configure one key source. |
| `GITHUB_POLL_INTERVAL_SECONDS`                                        | Poll interval, defaulting to 120 seconds.                                                   |
| `DATABASE_POOL_SIZE`                                                  | PostgreSQL connections per API process. Defaults to 10, with one reserved for updates.      |
| `SSE_MAX_STREAMS_PER_USER`, `SSE_MAX_STREAMS`                         | Concurrent update streams per user and per process. Default to 16 and 256.                  |
| `DATABASE_URL`                                                        | PostgreSQL connection URL. Required outside Compose.                                        |
| `HOST`, `PORT`                                                        | Server listener, defaulting to `127.0.0.1:3000` outside the container.                      |
| `API_PORT`, `POSTGRES_PORT`, `POSTGRES_PASSWORD`                      | Compose host ports and local PostgreSQL password.                                           |
| `AUTH_PROVIDERS`                                                      | Comma-separated `workos`, `entra` or `dev`.                                                 |
| `ENABLE_DEV_AUTH`                                                     | Explicit development sign-in opt-in. Local Compose sets this to `true`.                     |
| `WORKOS_CLIENT_ID`, `WORKOS_ISSUER`, `WORKOS_JWKS_URL`                | AuthKit client, issuer (default `https://api.workos.com/`) and optional JWKS override.      |
| `WORKOS_AUDIENCE`                                                     | Optional exact audience for AuthKit deployments that set an audience claim.                 |
| `WORKOS_API_KEY`                                                      | Optional profile lookup credential for the membership CLI.                                  |
| `ENTRA_TENANT_ID`, `ENTRA_CLIENT_ID`, `ENTRA_AUDIENCE`, `ENTRA_SCOPE` | Entra tenant, authorised client, API audience and delegated scope (default `API.Access`).   |
| `DEV_KEY_PATH`                                                        | Private development signing key file, defaulting to `.data/dev-signing-key`.                |
| `DEV_CHANNELS`, `DEV_IRC_HOST`, `DEV_IRC_PORT`                        | Channel text and IRC endpoint for newly created dev organisations.                          |
| `STORAGE_DRIVER`, `STORAGE_PATH`                                      | Defaults to `local` and `.data/blobs`. Use `s3` for a private S3-compatible bucket.         |
| `S3_BUCKET`, `S3_ENDPOINT`, `S3_REGION`                               | S3 bucket, optional custom endpoint and region.                                             |
| `S3_ACCESS_KEY_ID`, `S3_SECRET_ACCESS_KEY`, `S3_SESSION_TOKEN`        | Credentials and optional session token supplied to Bun's S3 client.                         |

The image defaults to `NODE_ENV=production`, which forbids development sign-in.
Local Compose defaults to `NODE_ENV=development`. Production deployments use TLS
for clients and the backing database and take credentials from runtime configuration.

Update streams return 429 at the configured limits and 503 while the listener
reconnects. Clients can retry with backoff and refresh their issue data over REST.

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
`serviceTokenClientId` and `ship` for captains. The ranks are `rearAdmiral`,
`captain` and `crew`. The `serviceTokenClientId` field carries the verified
provider subject for the agent in that organisation. Use the WorkOS `sub`,
the Entra `oid` or the email used by local dev sign-in. Authenticate as that
provider account to use the API. A bearer for a deactivated identity is refused
on subsequent requests. Humans can read organisation audit events. Agents can
read project audit events. Captains and rear admirals can attach crew name and
model to issue and comment writes. The authenticated agent remains the actor.

Issue creators, assignees and commenters follow issues automatically unless
they explicitly unsubscribe. A member can change their subscription under
`/api/v1/issues/{reference}/subscription` and manage their own inbox at
`/api/v1/notifications`. The API sends in-app notices for comments, linked
Markdown mentions and state changes.

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
organisations. The migration test creates an isolated database on the local PostgreSQL server named in `TEST_DATABASE_URL`. For signed pull request and deployment tests, sign in first as
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
