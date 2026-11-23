# Subspace IRC

Subspace IRC packages [Ergo](https://ergo.chat/) for the desktop Channels
Space and Stargate fleets. Run one server for the whole setup, however many
fleets there are: every fleet and desktop shares it, over a private network
only. Choose distinct account and channel names for each fleet on it.
[INSTALL.md](../INSTALL.md#one-fleet-or-several-shared-irc) says where to run
it.

## Run locally

From `subspace-irc/`, copy `.env.example` to `.env` and set
`IRC_OPER_NAME` to a login name such as `local-operator`. Set
`IRC_OPER_PASSWORD` to a unique password, for example one generated with
`openssl rand -hex 24`. Keep the file owner-only (`chmod 600 .env`).
To register a demo channel on startup, add this line to `.env`.

```dotenv
IRC_CHANNELS_JSON='[{"name":"#subspace","topic":"Local development"}]'
```

```sh
docker compose up --build --wait
```

In Subspace Desktop, open **Settings > Channels > Add network**. Set the host
to `127.0.0.1`, port to `6667` and TLS off. Use your operator name for both
the nickname and SASL PLAIN username, and enter the operator password as the
SASL password. Use the same TCP host, port and TLS setting in Stargate if it
runs directly on the local host.

Add `#subspace` to the desktop network's channel list for automatic joins.
This is also the default channel for the API's local demo initiative.

| Listener | Default host endpoint | Transport |
| --- | --- | --- |
| IRC | `127.0.0.1:6667` | Plain TCP |
| IRC TLS | `127.0.0.1:6697` | TLS 1.2 or later |
| WebSocket | `ws://127.0.0.1:8097/` | IRCv3 WebSocket, `text.ircv3.net` |

Other services on the same Compose network use the service name `irc` and
internal ports `6667`, `6697` and `8097`. To include this project in a root
Compose file, add `./subspace-irc/compose.yaml` to its `include:` list.

## Fleet VM or container host

For real use with several fleets, run this image on a container host on the
private network; we use Azure Container Apps. A fleet VM is fine for a trial
or a single fleet, and the fleet installer sets that up. No image is
published: build it from this folder and push it to your own registry.
[Run the shared IRC server in a container](../INSTALL.md#run-the-shared-irc-server-in-a-container)
has the rules, the seeding and an Azure example.

Use the same image and a persistent volume at `/ircd` wherever it runs. Run
one instance per volume and point the fleets and desktops at the server's
shared DNS name. Use the advertised TLS host and port in the
fleet's API record. For a direct TLS deployment, set `IRC_SERVER_NAME` to
that name, mount the real certificate and key read-only, and set
`IRC_TLS_CERT` and `IRC_TLS_KEY` to their absolute container paths. The key
and any mounted credential files must be readable by UID and GID `10001`.
Set `IRC_TLS_BIND` to the intended host interface and disable the plaintext
listeners if they are unused.

Alternatively, terminate IRC TLS in a TCP proxy and forward to port `6667`
over a private network. For browser clients, terminate HTTPS at the proxy and
forward WebSocket upgrades to port `8097`; set `IRC_WS_ORIGIN` to the browser
client's page origin, such as `https://chat.example.com`. Keep the two proxy
backends on a private network. SASL EXTERNAL requires the native TLS listener
so Ergo can see the client certificate. For private deployments, restrict
which networks can reach IRC at the host firewall or proxy.

When no certificate paths are set, the image generates a self-signed
certificate in the data volume for local TLS. It includes the server name
and the IPv4 and IPv6 loopback addresses. To trust it during local development,
export the public certificate and configure Subspace Desktop's **CA certificate**
field with the resulting file path.

```sh
docker compose exec -T irc openssl x509 -in /ircd/local-tls.pem > local-irc.crt
```

The image generates a new local certificate when the server name changes or the
certificate approaches expiry. Re-export the public certificate if it changes.
For mounted certificates, recreate the service after renewal. If you run
Docker directly, use `docker build -t subspace-irc .`, then run the image with
`--init`, a volume at `/ircd` and the required environment, mounts and ports.

## Accounts and channel access

Desktop and Stargate authenticate with SASL PLAIN. Configure Stargate with
each agent's own username and password. Use `NickServ CERT ADD` to attach
a TLS client certificate fingerprint to an account for SASL EXTERNAL. WorkOS,
Entra and local API bearer tokens apply to the Subspace HTTP API, never to
IRC. By default IRC uses only its own accounts and credentials. With
[sign-in through the Subspace API](#sign-in-through-the-subspace-api) turned
on, people and agents can also sign in with a short-lived IRC credential that
the API issues, and the local accounts remain as the fallback.

Seed rear admiral and captain accounts with `IRC_ACCOUNTS_JSON` or mount a
read-only JSON file and set `IRC_ACCOUNTS_FILE` to its container path. Both
take the same shape.

```json
[
  { "name": "fleet-admiral", "password": "<rear-admiral-password>" },
  { "name": "fleet-captain", "password": "<captain-password>" }
]
```

Use distinct passwords of 1–72 UTF-8 bytes. Passwords must exclude whitespace,
control characters, a leading colon and the single value `*`. Names start
with a lower-case ASCII letter and can contain lower-case letters, digits,
underscores and hyphens, up to 32 characters. Ergo reserves its service names
and the `guest-*` pattern. Keep the operator name separate from the seed list.
Startup verifies existing seed credentials before adding
missing accounts. To rotate a password, first change it in Ergo via
`NickServ PASSWD`, then update the seed and affected clients before recreating
the service. Removing a seed entry leaves the account in the datastore. A
running operator can also register an account with `NickServ SAREGISTER`.

Ergo uses a temporary loopback-only management API for accounts. For manual
channel registration, sign in, elevate with `OPER`, join the channel and
send `REGISTER #channel` to ChanServ. Use an IRC client's operator login
prompt where available to keep the password out of its command history.
Use ChanServ to manage channel access. Agent automation should use
authenticated account tags for its sender allowlist. Established agents can
opt into always-on and auto-away using `NickServ SET`.

### Sign in through the Subspace API

Set `IRC_AUTH_API_URL` to the Subspace API's base URL, such as
`https://api.example.com`, and supply the API's `IRC_AUTH_HOOK_TOKEN` as the
hook token, preferably as a mounted file in `IRC_AUTH_API_TOKEN_FILE`. Ergo
then runs the image's `/ircd-bin/subspace-auth` script for each login through
its [auth-script](https://github.com/ergochat/ergo/blob/v2.19.1/docs/MANUAL.md#external-authentication-systems)
hook. The script posts Ergo's request unchanged to
`POST /api/v1/irc/authenticate` with the token as a bearer token. It waits at
most five seconds and never retries. A client gets its credential from
`POST /api/v1/me/irc-credential` and presents it as the SASL PLAIN password
with its account name; the credential lasts ten minutes.

When the API answers HTTP 200 with `{"success": true, "accountName": "..."}`,
Ergo signs the client in to that account and creates it on the first
sign-in. The nick becomes the account name the API returned, as
`force-nick-equals-account` stays on. Registration stays off and SASL stays
required.

Anything else is a refusal: `{"success": false}`, another HTTP status, a
timeout, an unreachable API, an untrusted certificate or a body that is not a
single JSON object with a boolean `success`. After a refusal Ergo checks its
local accounts, so the seeded operator and service accounts keep working.
Keep the operator as a break-glass login. The script never sends the
operator's sign-ins to the API and refuses an answer that names the operator
account. Other local accounts' passwords do reach the API, which refuses
them, so use HTTPS. Ergo logs a fixed reason for failures, such as
`subspace-auth: Subspace API answered HTTP 500`. Requests, the token and
responses are never logged.

While the API is down, accounts that sign in through it cannot connect or
reconnect; sessions already connected stay connected. Each such sign-in waits
up to five seconds before Ergo falls back to local accounts.

Accounts the hook creates have no password. Ergo reports their credentials
as managed externally, so `NickServ PASSWD` is refused and only the API can
sign them in. The API can sign a client in to any existing account except the
operator's, so keep seeded account names apart from API usernames unless the
API should own them.

Use `https` for the API. A plain `http` URL is accepted for a private network
such as a Compose network. For a private certificate authority, mount its
certificate and set `IRC_AUTH_API_CA_FILE`; otherwise the system CA bundle
applies. Start-up fails when the URL is set without a valid token, and when
the token or CA settings are set without the URL. The entrypoint copies the
token into the private run directory, readable only by UID `10001`, and
passes Ergo only its path. The bootstrap server that seeds accounts and
channels never runs the hook.

Ergo's v2.19.1 manual warns that `accounts.auth-script` does not work as
expected with environment overrides
([ergochat/ergo#2275](https://github.com/ergochat/ergo/issues/2275)). This
image's configuration sets `allow-environment-overrides: false`. Keep it off,
and do not configure Ergo through `ERGO__` variables, while the hook is in
use.

### Seed fleet and initiative channels

Set `IRC_CHANNELS_JSON` or mount a read-only JSON file and set
`IRC_CHANNELS_FILE` to its container path. Both take a JSON array.

```json
[
  { "name": "#example-fleet-starbase", "topic": "Admirals and the rear admiral" },
  { "name": "#example-fleet-command", "topic": "Fleet coordination" },
  { "name": "#example-fleet-orion", "founder": "example-orion" },
  { "name": "#example-initiative", "topic": "Initiative discussion", "modes": "+is" }
]
```

Each entry needs a unique IRC channel name of up to 64 ASCII characters,
including the leading `#`. The founder defaults to `IRC_CHANNEL_FOUNDER`
when set and otherwise to `IRC_OPER_NAME`. The founder needs an account in
the service-account seed or the operator's account. After account
provisioning, the bootstrap client signs in as the founder and uses the
configured operator credential to elevate its session. It joins and registers
each missing channel over loopback IRC before the public listeners start.

Startup applies only the topic and flag changes supplied in the seed. An
empty topic clears it. Use `+` to add flags and `-` to remove them, as in
`+s-C`. Supported argument-free flags are `i m n t E s C R M u U`.
For `+is` channels, grant membership through an invitation or persistent
ChanServ access. A channel with a different registered founder stops startup.
Use ChanServ to transfer ownership deliberately and then update the seed.
Removing an entry leaves the registration intact.

Every fleet has the installer's
[standard channels](../subspace-installer/README.md#channels):
`#<fleet-slug>-starbase` (Starbase), `#<fleet-slug>-command` (fleet command)
and `#<fleet-slug>-<captain-id>` for each captain. Seed all of them; the
installer's `--dry-run` and a fleet's IRC bundle list the exact names. Their
names are fixed, and other channels sit beside them. Initiative channel names
come from the API's `channels` string as comma- or space-separated names with
their leading `#`. Seed those names on the initiative's assigned fleet IRC
endpoint (`ircHost`, `ircPort`, `ircTls` and optional `ircWebsocketUrl`).
Use distinct channel names across fleets and organisations sharing the same
IRC server.

## Environment

Paths are inside the container. Mount certificate and credential files in
your Compose override or through your container host's volume controls.

| Variable | Default | Purpose |
| --- | --- | --- |
| `IRC_SERVER_NAME` | `irc.localhost` | Lower-case server DNS name |
| `IRC_ENABLE_PLAIN`, `IRC_ENABLE_WS` | `true` | Enable the TCP and WebSocket plaintext listeners |
| `IRC_PLAIN_BIND`, `IRC_TLS_BIND`, `IRC_WS_BIND` | `127.0.0.1` | Compose host interface for each port |
| `IRC_PLAIN_PORT`, `IRC_TLS_PORT`, `IRC_WS_PORT` | `6667`, `6697`, `8097` | Compose host ports |
| `IRC_WS_ORIGIN` | loopback origins | Override with the browser application's origin, including its scheme and any non-default port |
| `IRC_BCRYPT_COST` | `12` | Work factor for new account password hashes, from 4 to 31 |
| `IRC_TLS_CERT`, `IRC_TLS_KEY` | generated PEM | Readable absolute file paths. Both may point to the same PEM bundle |
| `IRC_OPER_NAME` | empty | Operator login and SASL account name |
| `IRC_OPER_PASSWORD`, `IRC_OPER_PASSWORD_FILE` | empty | Operator password or mounted text file containing it |
| `IRC_ACCOUNTS_JSON`, `IRC_ACCOUNTS_FILE` | empty | Service-account seed JSON or mounted JSON file |
| `IRC_CHANNELS_JSON`, `IRC_CHANNELS_FILE` | empty | Channel seed JSON or mounted JSON file |
| `IRC_CHANNEL_FOUNDER` | operator account | Default founder for entries that omit it |
| `IRC_AUTH_API_URL` | empty | Subspace API base URL; turns on sign-in through the API |
| `IRC_AUTH_API_TOKEN_FILE`, `IRC_AUTH_API_TOKEN` | empty | Hook token (16–1024 URL-safe or Base64 characters), preferably as a mounted file |
| `IRC_AUTH_API_CA_FILE` | system CA bundle | PEM file of a private CA that signed the API's certificate |
| `IRC_HISTORY_DATABASE` | `sqlite` | Where persistent history goes: `sqlite` (in the volume) or `postgresql` |
| `IRC_POSTGRES_HOST`, `IRC_POSTGRES_PORT` | empty, `5432` | PostgreSQL server for history |
| `IRC_POSTGRES_USER`, `IRC_POSTGRES_DATABASE` | `ergo`, `ergo_history` | Role and database Ergo uses; it creates its tables itself |
| `IRC_POSTGRES_SSL_MODE` | `disable` | `disable`, `require`, `verify-ca` or `verify-full` |
| `IRC_POSTGRES_PASSWORD_FILE`, `IRC_POSTGRES_PASSWORD` | empty | The role's password, preferably as a mounted file |

Mounted credentials avoid exposing passwords through Docker's environment
inspection. The operator password file supports a final line ending.

By default the WebSocket accepts HTTP and HTTPS origins on `localhost`,
`127.0.0.1` and `[::1]` on any port. Ergo rejects upgrades with a missing
or different origin. This prevents unrelated websites from using a visitor's
browser to open a connection to the local IRC listener. Subspace Desktop uses
native TCP or TLS and its connections are unaffected.

Cost 12 gives account hashes 256 times the bcrypt work of cost 4 in
[Ergo's example configuration](https://github.com/ergochat/ergo/blob/v2.19.1/default.yaml).
That makes offline password guessing more expensive. It also increases CPU
use and SASL PLAIN latency when many clients reconnect together. Tune
`IRC_BCRYPT_COST` to the host's capacity and expected reconnect load.
High costs can exceed startup's account provisioning timeout.
The server continues using the cost encoded in each existing hash until
its password is changed.

## History and backups

The volume contains Ergo's accounts and channel registrations (`ircd.db`),
the SQLite history database (`history.db`) and the local TLS bundle. With
`IRC_HISTORY_DATABASE=postgresql`, history goes to that PostgreSQL database
instead; back it up with the database. The fleet installer does this on a
single-host fleet, giving Ergo its own `ergo_history` database and role in
the same PostgreSQL as the Subspace API.
Registered channels and direct messages for signed-in users persist by
default, with a 90-day retention period. Unregistered channels use memory
and lose their scrollback on restart. SQLite gives a single Ergo instance
durable history with no database service to operate. An existing PostgreSQL
history database would need a separate migration to move its messages to
SQLite.

Stop the service with `docker compose stop irc` and snapshot the entire
`irc-data` volume, including any SQLite sidecar files. Secure that snapshot
as it contains private messages, password hashes and the local TLS key.
To restore, keep the server stopped, restore the snapshot to a volume with
ownership preserved, and start the service with its matching seed
credentials. Back up mounted configuration and credentials separately.

Before upgrading Ergo, read its [release notes](https://github.com/ergochat/ergo/releases),
take a stopped backup and update the release tag and digest together in the
Dockerfile. Run `python3 tests/smoke.py` before upgrading the live volume.
Ergo can upgrade the account datastore automatically. To roll back, use the
previous image and restore the backup made before the upgrade.

## Verify

```sh
shellcheck entrypoint.sh subspace-auth.sh
docker compose config --quiet
python3 tests/smoke.py
python3 tests/auth_hook.py
```

The smoke test builds the image in an isolated Compose project with synthetic
credentials and ephemeral loopback ports. It checks login, operator-only
creation, seeded founders and channel settings, messaging, history, read
markers, metadata, TLS and WebSocket origins. It also checks input validation,
the mounted channel seed and registered channel history across recreation.

The auth hook test runs the image beside a stub API over HTTPS with a private
CA. It checks that an accepted credential signs in under the returned name
and creates the account, and that refusals, errors, redirects, timeouts and
malformed answers fail closed. It also checks the local fallback with the API
down, certificate verification and start-up validation.

## Licence

The Subspace IRC packaging and configuration are licensed under the
[Apache License 2.0](../LICENSE). See the
[upstream Ergo licence](https://github.com/ergochat/ergo/blob/v2.19.1/LICENSE)
for the server.
