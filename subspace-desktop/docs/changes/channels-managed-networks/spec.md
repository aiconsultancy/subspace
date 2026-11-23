# Spec: networks from the signed-in account

Back to the [record](README.md). The behaviour and its trust model are
described for maintainers in
[Networks from your account](../../irc.md#networks-from-your-account).

## The contract consumed

- `GET /api/v1/me` may carry `username` (a string). An API that predates it
  sends none, which brings no network.
- `POST /api/v1/me/irc-credential`, with the same bearer as every other
  call, takes `{ "host": string, "port": integer }`, the IRC server the login
  is for, and answers `{ "account", "password", "expiresAt" }` for a server
  the API vouches for. The password is opaque, valid on that host alone for
  about ten minutes and reusable until then. A server the API does not vouch
  for, or an API without the feature, answers 404.
- Fleets carry `ircHost`, `ircPort` and `ircTls`; initiatives carry their
  fleet.

## API client (`clients/subspace-api`)

- `model::Me::username: Option<String>`.
- `Client::irc_credential(host, port)` posts `{host, port}` once to
  `v1/me/irc-credential`, never retried; an empty host or port 0 is refused
  before anything is sent. The answer is an `IrcCredential` whose password is
  a `Secret`, so formatting it prints `***`. An account that is not one IRC
  name (ASCII letters, digits and nick punctuation, starting with a letter,
  at most 64 bytes, no `@`), a password that is empty, longer than 400 bytes
  or not printable ASCII, or an expiry that is not RFC 3339 is refused as
  `InvalidResponse`. 404 is `NotFound`; 401 is `SignInNeeded`; a transport
  failure is `Network`. A NETT client answers `NotFound` without asking.
- `Catalog::verified_for()` names the account generation and verified user
  the memberships were read for, while the latest read succeeded for the
  current scope. `signed_in_generation()` is the current account generation.

## IRC client (`clients/irc`)

- `Sasl::credential: Option<CredentialSource>`, `#[serde(skip)]`: never read
  from or written to a settings file. `CredentialSource` wraps a blocking
  `Fn() -> Result<IssuedLogin, String>` and a key (sources with equal keys
  are equal, so rebuilding one restarts nothing), and `on_refused` tells the
  owner when a server did not accept its login.
- `IssuedLogin { nickname, username, password: IssuedPassword }`.
  `IssuedPassword` never formats (`***`), has no `Serialize` and never enters
  the network's configuration.
- At every connect, after the TCP and TLS handshake, the network task
  resolves the login: the inline password, else the password file, else the
  source, asked on a blocking thread. Disconnect and shutdown abandon the
  wait, as they abandon a connect. A certificate the client cannot verify
  ends the connect before the source is asked.
- Without TLS, a network with a credential source is refused (`Failed`)
  unless the connected peer is a loopback address, before the source is
  asked.
- The issued login sets the session's nick and SASL username; its password
  goes to the session apart from the configuration, is sent in the one
  PLAIN exchange and dropped. SASL outcomes for such a session are fixed
  words with the numeric (`The server did not accept the sign-in (904)`).
- Such a session must sign in: a SASL refusal (902, 904 to 906) ends the
  link at once, and 001 without a 903 sends `QUIT` and joins nothing;
  either way the network retries as a lost connection with its backoff, and
  the source's `on_refused` runs. `IssuedPassword::while_valid` is asked
  before the password is sent; a lapsed login is not sent.
- A source error is retried as a lost connection (`Reconnecting` with the
  error as its reason); a password file error stays final (`Failed`).

## Channels (`spaces/irc`)

- **Environment** (`managed/env.rs`): the client and the account
  generation come from `Env`, the app's (`Client::new`,
  `signed_in_generation`) or a test's.
- **Account check** (`managed/account.rs`): on the hub's timer, when no
  check has settled for the current account, a background task reads `/me`.
  `SignedIn { id, username }` with a username; `Without` for no username or
  404; 401 and 403 are checked again after 5 s doubling to 30 min, other
  failures to 5 min. The account generation is compared at most once a
  second; a change forgets the account, its servers, logins and networks
  and checks again. An answer begun in another generation is dropped.
- **Memberships**: nothing is planned until `Catalog::verified_for()` names
  the checked generation and user. While signed in and they are not, the
  catalogue is read again at once, then after 5 s doubling to 5 min.
- **Servers** (`managed/plan.rs`, `initiatives/endpoint.rs`): the fleets'
  servers, each once by host, port and TLS, named after their fleets. Every
  host comparison uses one form on both sides (`comparable_host`: ASCII
  lower case, no final dot or IPv6 brackets, canonical IP). Only servers a
  login may go to count (`Endpoint::sendable`: a DNS name or IP literal, no
  whitespace, control characters, `/`, `@` or port, and a port from 1 to
  65535), over TLS or, without it, named as loopback.
- **Probes** (`managed/servers.rs`): each server's login is asked for once
  with its host and port. `Vouched(account)` on success; `Refused` on 404,
  for that server only; anything else is asked again after 5 s doubling (to
  30 min for 401 and 403, else 5 min). A server the catalogue drops goes
  with its login; a late answer for it changes nothing.
- **Logins** (`managed/login.rs`): per server, the issued login lasts
  `valid_for` from when it was received (`expiresAt` less the API's `Date`)
  and is kept until a minute before it lapses; one that arrives expired is
  refused; the API is asked no sooner than 10 s after the last request, or
  after a wait from 5 s doubling to 5 min after a failure, one request at a
  time, without holding the lock while it answers; a failed renewal keeps
  offering the unexpired login. The probe's login is the first connect's,
  and each issued password is checked for expiry and the account when it
  would be sent.
- **Credential source** (`managed/source.rs`): one per network, keyed by API
  origin, user and server, offering only its server's login, refusing once
  its account generation is over, and dropping its login when the server
  refused it. Errors read `Subspace API could not be reached for an IRC
  sign-in`, `Sign in under Settings › Account to connect`, `Subspace API does
  not issue sign-ins for this IRC server`, `Subspace API could not issue an
  IRC sign-in`, `Subspace API issued an IRC sign-in that had expired` or
  `Waiting before asking Subspace API for an IRC sign-in`.
- **Networks**: one per vouched server that no saved network serves (same
  host, ignoring case and surrounding space, and the same port, whatever the
  TLS settings). Id `@account.<person>.<server>`, 16 hexadecimal digits each
  of SHA-256 of the API's user ID and of `host|port|tls`; name the fleets'
  names; host, port and TLS from the fleet; certificate checks on; nick the
  issued account; no channels; connect on start; SASL username
  `account@device`, empty password, the server's credential source.
- **Saved ids** (`managed/ids.rs`): only a saved id that starts with
  `@account.` is skipped when the settings load, with the reason, and named
  in Settings, which refuses Save until it has another id; only an id of
  exactly the account form is an account's.
- **History**: nothing is deleted when the account changes. Account
  networks run only for their person, so only theirs are listed; search
  hides other people's account network lines (`Service::set_hidden`, with
  `ids::hidden`); and when an account network starts running after the
  restore at start, its stored buffers are read again and restored with
  their anchors.
- **Device** (`managed/device.rs`): `irc.device` in the state file, 16
  lower-case hexadecimal digits from a v4 UUID, made when missing or not of
  that form.
- **Hub** (`hub/account.rs`): the effective settings are the saved ones,
  then the account's networks; `apply` stores the saved settings and
  reconciles, and `follow` applies the effective ones, so unchanged networks
  keep their connection.
- **Settings** (`editor/account_view.rs`): while the account brings
  networks, **From your account** introduces them with the issued account
  name and the service name (`service_name()`), then lists each vouched
  server, `host:port · TLS · account`, and `Not connected: your network
  "name" connects to this server instead.` when a draft network serves it;
  the saved list is then headed **Saved networks**, empty **No saved
  networks yet.** A draft network on one of those servers says `Your Subspace
  API account can sign in to this server for you.` and gains **Use account
  sign-in…**, which asks **Switch "name" to your account?** with **Cancel**
  and **Switch**. A change of the account's networks withdraws the question;
  Switch is disabled on a locked page and checks again that the network
  still serves an account server before it removes it from the draft, and
  Save stops with the reason if the person who switched it signed out,
  another account signed in, the memberships cannot be confirmed or the
  server is no longer the account's.

## Left out

- Choosing an initiative is still how its channels are joined; a fresh
  profile with one initiative connects at once but joins its channels once
  the initiative is picked. Selecting a person's only initiative by itself
  would change the shared picker for Issues too.
- NETT does not issue IRC credentials yet.
- A plaintext fleet server off this computer gets no network.
- Fixed SASL wording applies to sessions with an issued login; a saved
  network keeps the server's words, as the IRC client's clean-room
  acceptance tests require.
