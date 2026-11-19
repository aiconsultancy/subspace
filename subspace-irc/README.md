# Subspace IRC

Subspace IRC packages [Ergo](https://ergo.chat/) for the desktop Channels
Space and Stargate fleets. One server can serve many fleets and desktops.
Choose distinct account and channel names for each fleet on a shared server.

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

Use the same image and a persistent volume at `/ircd` on a fleet VM or a
separate host. Run one instance per volume and point the fleets and desktops
at the server's shared DNS name. Use the advertised TLS host and port in the
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
Entra and local API bearer tokens apply to the Subspace HTTP API. IRC uses
its own accounts and credentials.

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

### Seed fleet and initiative channels

Set `IRC_CHANNELS_JSON` or mount a read-only JSON file and set
`IRC_CHANNELS_FILE` to its container path. Both take a JSON array.

```json
[
  { "name": "#example-command", "topic": "Fleet coordination" },
  { "name": "#example-captain", "founder": "example-captain" },
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

Stargate and the installer use `#<fleet-slug>-command` and
`#<fleet-slug>-<captain-id>` by default. The command channel serves as the
fleet's Starbase. Use the exact names from the gateway configuration in the
seed, including any overrides. Initiative channel names come from
the API's `channels` string as comma- or space-separated names with their
leading `#`. Seed those names on the initiative's assigned fleet IRC
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
the SQLite history database (`history.db`) and the local TLS bundle.
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
shellcheck entrypoint.sh
docker compose config --quiet
python3 tests/smoke.py
```

The smoke test builds the image in an isolated Compose project with synthetic
credentials and ephemeral loopback ports. It checks login, operator-only
creation, seeded founders and channel settings, messaging, history, read
markers, metadata, TLS and WebSocket origins. It also checks input validation,
the mounted channel seed and registered channel history across recreation.

## Licence

The Subspace IRC packaging and configuration are licensed under the
[Apache License 2.0](../LICENSE). See the
[upstream Ergo licence](https://github.com/ergochat/ergo/blob/v2.19.1/LICENSE)
for the server.
