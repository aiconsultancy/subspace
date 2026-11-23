# Delivery: networks from the signed-in account

Back to the [record](README.md).

## Checks

Run from `subspace-desktop` on the branch, with Cargo through mise, on
23 November 2026:

- `make check` passed: architecture, board-guide and documentation-index
  checks, format, cargo-shear, clippy with warnings denied, the tests (55
  test binaries, none failed, over the workspace with the unreleased Spaces
  and the app in its release configuration: 2,988 tests at first, 3,017
  after the first review round) and the script fixtures,
  `scripts/test-capture.sh` among them.
- `mise run quality` passed: 100.0% against `rustqual-baseline.json`, no
  new violations, and `rustqual . --findings` reports none.
- `python3 -I scripts/test-subspace-api-test-server.py` passed (31 tests),
  with the credential route asked for one server.
- `mise run audit` reports advisories (`cookie`, `heapless`, `url` and
  others) for crates already in the lockfile on `main`; this change adds
  no crate, only `uuid`, already locked, to the Channels Space.

New tests (with the first review round's):

- API client (`clients/subspace-api/src/tests/irc_credential.rs`): one POST
  with the bearer naming the server, nothing sent without a host or port,
  the password never formatted, 404, 401, 500 and 503, an unreachable API,
  credentials an IRC line cannot carry, `username` present and absent, NETT
  never asked, and the catalogue naming the account it was read for.
- IRC client sign-in (`session/tests/sign_in.rs`,
  `network/tests/credential.rs`): the issued password sent once then
  dropped, fixed SASL wording, a server registering without accepting the
  sign-in left before any join and retried with the source told, a
  certificate failure never asking for a login, and a plaintext login only
  to a loopback peer.
- Channels per server (`managed/login.rs`, `managed/servers.rs`,
  `managed/tests.rs`, `managed/tests/lifecycle.rs`,
  `managed/tests/history.rs`): logins kept until a minute before expiry,
  spaced and backed off, refused when expired; servers vouched or refused
  on their own; forty connects using two probe logins, each network only its
  own server's; a switch with the old memberships still loaded; check and
  probe answers after a switch dropped; a source refusing after sign-out;
  servers coming and going; memberships read again; history restored for
  the account and deleted for another person; endpoint validation; saved
  ids in the reserved namespace refused on load and in Settings.
- IRC client (`network/secrets.rs`, `network/tests/credential.rs`,
  `session/tests/registration.rs`, `model/credential.rs`): the source asked
  at each connect and reconnect and setting the nick, a saved password or
  EXTERNAL never asking it, a failing source retried and never failed while
  a bad password file still fails, PLAIN sending `account@device` as both
  identities, and sources comparing by key without showing a password.
- Channels (`managed/*`, `managed/tests.rs`, `editor/tests/account.rs`):
  the account check's outcomes and backoff, server and network derivation
  (distinct servers, plaintext servers elsewhere left out, the login's
  shape, saved networks on the same server, ids), the device ID, the
  source's requests and reasons; over a hub, sign-in bringing a network
  beside the saved ones, the initiative's autojoin on it, sign-out removing
  it, a Settings save leaving it connected, a saved network on the same
  server kept with no second connection, an API without sign-ins bringing
  none, and Settings seeing it only while signed in; headless renders of the
  read-only section, its absence, and the switch offer, its cancellation and
  its confirmation.

## Screenshots

Captured with `make capture` from the branch's capture build (and
`origin/main` at `9d53e2b` for the before image) in Twilight Two,
1440 × 900, with the fixture API's aurora fleet (`CAPTURE_FLEET=1`) on the
fictional `irc.example.test:6697`, which never resolves, so its connection
retries. The signed-in fixture person is `ada`. The fixture vouches for the
aurora fleet's server only. After the first review round the account,
clash, switch and Channels images were captured again from the branch's
head: the saved list is headed **Saved networks**, and the words name the
service and the Space rather than the product.

Settings › Channels before, with a saved network on the fleet's server
(`origin/main`):

![Settings › Channels on main, with one saved network](evidence/settings-channels-before.png)

After, against an API without IRC sign-ins (`CAPTURE_IRC_CREDENTIAL` off):
unchanged.

![Settings › Channels unchanged when the API issues no IRC sign-ins](evidence/settings-channels-without-credentials.png)

After, signed in with IRC sign-ins and no saved network
(`CAPTURE_IRC_CREDENTIAL=1`): the account's network, read-only.

![Settings › Channels listing the aurora fleet's server under From your account](evidence/settings-channels-account.png)

After, with a saved network on the same server: it stays, with the offer to
switch, and the account's row says why it is not connected.

![A saved network on the fleet's server, offered Use account sign-in…](evidence/settings-channels-clash.png)

After choosing **Use account sign-in…**: the confirmation.

![Switch “Fleet IRC” to your account? with Cancel and Switch](evidence/settings-channels-switch.png)

Channels with no saved network: the account's network for the aurora
fleet, retrying because the fictional server does not resolve.

![Channels showing the Aurora network from the account, reconnecting](evidence/channels-account-network.png)

## Review round one

Two reviews of pull request 83 found the issues below; each was checked
against the code and fixed in its own commit.

- **The credential was not bound to a server** (P1): the desktop asked for
  one login and offered it to every fleet server. The contract now takes
  the server's host and port; each server is probed and signed in with its
  own login, and a 404 removes only that server.
- **An account switch could plan from the previous account's fleets** (P1):
  planning now waits for memberships read for the same generation and
  verified user.
- **A saved network could take an account network's id** (P1): account
  networks have `@account.` ids from digests of the user ID and endpoint,
  refused in saved settings; their history is restored with them and other
  people's is deleted at sign-in.
- **The issued password was in the configuration** (P2): it is an
  `IssuedPassword`, sent once and dropped, with fixed SASL wording.
- **Expiry ignored and a request per reconnect** (P2): logins are kept
  until a minute before expiry, an expired one is refused, and requests are
  spaced with backoff.
- **SASL not required** (P2): registration without an accepted SASL
  exchange disconnects before any join and retries.
- **Endpoints not validated; `localhost` trusted by name** (P2): hosts and
  ports are validated before use, and a plaintext login needs a loopback
  peer; a certificate failure prevents any request.
- **Refusals settled for the session; unread memberships stayed empty**
  (P2): only 404 or a missing username settles; refusals and failed reads
  are retried with backoff.
- **Settings** (P2, P3): the switch is withdrawn on change, revalidated and
  disabled on a locked page; the words use `service_name()` and the issued
  account; the saved list is labelled apart.
- **Tests for the real path** (P3): sign-in through the timer, late answers
  after a switch, a source refusing after sign-out, servers coming and
  going, and memberships read again.

## Not verified

- **End-to-end sign-in.** No Subspace API with the credential route and no
  Ergo server with the auth-script hook exist yet, so a real sign-in, Ergo's
  account creation and per-device history replay were not exercised. The
  client was tested against the written contract with offline fixtures, and
  PLAIN's payload against Ergo 2.19.1's rule that both identities match
  before the device is removed.
- **The retrying reason on screen.** The capture's fleet server does not
  resolve, so its network retries for that reason, not for a missing
  credential; the credential reasons are covered by the IRC client's
  network tests and the source's tests.
- **Identity providers.** Only the API calls differ between `dev`, `workos`
  and `entra`, and the capture's debug fixture stands in for all three; no
  real provider was signed in to.
