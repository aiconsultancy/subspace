# Spec: networks from the signed-in account

Back to the [record](README.md). The behaviour is described for maintainers
in [Networks from your account](../../irc.md#networks-from-your-account).

## The contract consumed

- `GET /api/v1/me` may carry `username` (a string). An API that predates it
  sends none, which brings no network.
- `POST /api/v1/me/irc-credential`, with the same bearer as every other
  call, answers `{ "account", "password", "expiresAt" }`. The password is
  opaque, valid for about ten minutes and reusable until then. An API
  without the feature answers 404.
- Fleets carry `ircHost`, `ircPort` and `ircTls`; initiatives carry their
  fleet.

## API client (`clients/subspace-api`)

- `model::Me::username: Option<String>`.
- `Client::irc_credential()` posts `{}` once to `v1/me/irc-credential`,
  never retried. The answer is an `IrcCredential` whose password is a
  `Secret`, so formatting it prints `***`. An account that is not one IRC
  name (ASCII letters, digits and nick punctuation, starting with a letter,
  at most 64 bytes, no `@`), a password that is empty, longer than 400 bytes
  or not printable ASCII, or an expiry that is not RFC 3339 is refused as
  `InvalidResponse`. 404 is `NotFound`; 401 is `SignInNeeded`; a transport
  failure is `Network`. A NETT client answers `NotFound` without asking.

## IRC client (`clients/irc`)

- `Sasl::credential: Option<CredentialSource>`, `#[serde(skip)]`: never read
  from or written to a settings file.
- `CredentialSource` wraps a blocking `Fn() -> Result<IssuedLogin, String>`
  and a key; two sources are equal when their keys are, so an owner can
  rebuild one without the runtime restarting the network.
- At every connect, after the TCP and TLS handshake where a password file is
  read, the network task resolves the login: the inline password, else the
  password file, else the source, asked on a blocking thread. Disconnect and
  shutdown abandon the wait, as they abandon a connect. The issued login
  sets the nick and the SASL username and password of the session only.
- A source error is retried as a lost connection (`Reconnecting` with the
  error as its reason, with the usual backoff); a password file error stays
  final (`Failed`).
- PLAIN sends the username as both identities, so `account@device` passes
  Ergo's check that they match before it takes the device off.

## Channels (`spaces/irc`)

- **Account check** (`managed/account.rs`): on the hub's timer, when no
  check has settled for the current account, a background task builds a
  `Client`, reads `/me` and, with a username, asks for one credential and
  drops it. `SignedIn { username, account }` when both work; `Without` for
  no username, 404, a refusal or a sign-out; a network failure, an
  unexpected status or an invalid answer is checked again after 5 s,
  doubling up to 5 min. The account is compared with the checked client's
  at most once a second; a change forgets the account and its networks and
  checks again. An answer that arrives after the account changed is
  discarded.
- **Servers** (`managed/plan.rs`): the fleets of the catalogue's
  memberships, each server once by host (ASCII case ignored), port and TLS,
  named after its fleets. Only TLS or loopback servers count.
- **Networks**: one per server that no saved network serves (same host,
  ignoring case and surrounding space, and the same port, whatever the TLS
  settings). Id `slug(account host port)`, made unique against saved ids;
  name the fleets' names; host, port and TLS from the fleet; certificate
  checks on; nick the account; no channels; connect on start; SASL username
  `account@device`, empty password, the credential source.
- **Credential source** (`managed/source.rs`): keyed by API origin and
  account, bound to the client of the check, so after a sign-out or switch
  it is refused. Its errors read `Subspace API could not be reached for an
  IRC sign-in`, `Sign in under Settings › Account to connect`, `Subspace API
  does not issue IRC sign-ins` or `Subspace API could not issue an IRC
  sign-in`.
- **Device** (`managed/device.rs`): `irc.device` in the state file, 16
  lower-case hexadecimal digits from a v4 UUID, made when missing or not of
  that form.
- **Hub** (`hub/account.rs`): `saved` holds the file's settings; the
  effective settings are the saved ones then the account's networks.
  `apply` (a Settings save) stores the saved settings and reconciles;
  catalogue changes and a settled check reconcile too; `follow` applies the
  effective settings as `apply` did before, so unchanged networks keep their
  connection. After a check signs in and the memberships are not known, the
  catalogue is read.
- **Settings** (`editor/account_view.rs`): a **From your account** section
  while the account brings networks (a username and at least one server):
  an introduction naming the username, then per server its name and
  `host:port · TLS · username`, and `Not connected: your network "name"
  connects to this server instead.` when a draft network serves it. A draft
  network on one of those servers says `Your Subspace account can sign in to
  this server for you.` and gains **Use account sign-in…**, which asks
  **Switch "name" to your account?** with **Cancel** and **Switch**; Switch
  removes it from the draft, and Save writes the rest. The editor reads the
  hub when it opens and follows it.

## Left out

- Choosing an initiative is still how its channels are joined; a fresh
  profile with one initiative connects at once but joins its channels once
  the initiative is picked. Selecting a person's only initiative by itself
  would change the shared picker for Issues too.
- NETT does not issue IRC credentials yet.
- A plaintext fleet server off this computer gets no network.
