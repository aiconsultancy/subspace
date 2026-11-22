# Fleets and the Fleet API

Each fleet VM runs Stargate, whose private Fleet API serves the fleet's live
state: the rear admiral, the ships with their captains' queues and crew, each
crew member's current work, and the host's figures. The desktop reads it from
native code through `clients/fleet` (crate `subspace-client-fleet`), and the
built-in [Fleet Console](../web-spaces/fleet-console/README.md) reads it from
its page. Back to the [documentation index](README.md).

## Where a fleet's address and token come from

- **The address** comes from the Subspace API. A fleet record carries an
  optional `fleetApiUrl`: an absolute `https` URL, or `http` on loopback,
  without credentials, query or fragment, normalised without a trailing
  slash. The desktop checks it again (`FleetAddress::parse`) and treats an
  address that fails the check as not usable: it sends no token there.
- **The token** is the person's own, saved on their machine once per fleet,
  in **Settings › Fleets**. It is never stored in or sent to the Subspace
  API. Every Space that shows a fleet uses the same saved token: native
  Spaces through the fleet service, and the built-in Fleet Console through
  its bridge.

## Where tokens are stored

Tokens are kept in the app's secret vault
(`subspace_framework::core::vault`), the same store as signed-in GitHub
accounts:

| Build | Store | Name |
| --- | --- | --- |
| Release | The system keychain (macOS Keychain, Windows Credential Manager, Secret Service) | service `<app>.fleet-api` (for the stable app `subspace.fleet-api`), one account per fleet: `fleet-api:<fleet id>` |
| Debug, capture | An owner-only JSON file beside `state.json` | `fleet-api-tokens.json`, keys `fleet-api:<fleet id>` |
| Tests | Memory, or a file the test names | |

A debug build uses the system keychain instead when
`SUBSPACE_DEVELOPMENT_USE_KEYCHAIN` is set, as for other debug-build secrets.

Each value is `{"v":1,"endpoint":"<address>","token":"<token>"}`: the token
together with the normalised address it was saved for, the same form the
Fleet Console uses for the fleets added in its page.

## The address rule

A token is only ever sent to the address it was saved for. When the address
the Subspace API records for a fleet differs from the one its token was saved
for, the token is not sent anywhere: the fleet's state is **Address changed**,
Settings › Fleets says which address the token was for, and the person
confirms the new address by pasting the token again, which saves it bound to
the new address. A token saved without an address is never used either.

Requests never follow redirects and never use a proxy. The token travels
only in the `Authorization` header (marked sensitive on the stream); it is
never in a URL, a log line, an error message or UI state.

## The fleet service

`FleetApi::shared(cx)` returns the process's one service, a GPUI entity. It
follows the initiatives catalogue (`subspace_api_client::initiatives::Catalog`)
for the fleets the person's initiatives belong to, and asks it to read them
when it has none yet, so a Space shown first, such as Fleet Console, gets
the fleets without another Space loading them. It reads each fleet's saved
token in the background and reports each fleet's `FleetState`:

| State | Meaning |
| --- | --- |
| `NotConfigured` | The Subspace API records no Fleet API address. |
| `InvalidAddress` | The recorded address is not one a token is sent to. |
| `Checking` | The saved token is being read. |
| `NoToken` | No token is saved on this computer. |
| `AddressChanged` | The token was saved for another address. |
| `StoreUnavailable` | The keychain could not be read. |
| `Ready` | A token is saved for the address; nothing follows the fleet. |
| `Followed(Link)` | Something follows the fleet. `Link` is `Connecting`, `Live`, `Polling` (Stargate refused the stream; read every 15 s), `Reconnecting`, `TokenRefused`, `Forbidden` (Stargate refused this network) or `Unreachable`. `FleetState::is_live()` is true for `Live` and `Polling`. |

Each state has a short `label()` and, where the person can act, a `help()`
sentence. `FleetState::NotConfigured` is also the state of a fleet that is
not one of the initiatives'.

A Space keeps a `FleetFollow` handle while it shows a fleet. Every handle for
a fleet shares one event stream, so the process opens at most one stream per
fleet however many Spaces follow it; Stargate serves at most eight streams
per gateway. Dropping the last handle closes the stream.

```rust
use subspace_client_fleet::{FleetApi, FleetFollow, FleetSnapshot, FleetState, Link};

let fleets = FleetApi::shared(cx);
self.follow = Some(fleets.update(cx, |fleets, cx| fleets.follow(&fleet_id, cx)));
cx.observe(&fleets, |this, fleets, cx| {
    let fleets = fleets.read(cx);
    this.state = fleets.state(&this.fleet_id);
    this.snapshot = fleets.snapshot(&this.fleet_id); // overview, ship details, activity
    cx.notify();
})
.detach();
// this.state == FleetState::Followed(Link::Live): this.snapshot.overview has
// the fleet, this.snapshot.ships each ship's crew, this.snapshot.activity
// the latest lines.
```

`FleetApi::fleets()` lists every fleet as a `FleetInfo` (`record` with the
ID, name and recorded address, the checked `address` and the `state`);
`FleetApi::revision()` changes with any news and
`FleetApi::directory_revision()` only when a fleet, its address or its token
changes.

Channels follows each fleet its navigator (while the navigator shows) or
the conversation in front shows, while the Space is in front and the fleet is `Ready` or followed,
and drops the handle as soon as it no longer shows it; it reads the
overview and each ship's detail for a ship's dot, its header and its
inspector ([Live fleet state](irc.md#live-fleet-state)). A Space that must
not open the token store, such as one under test, reads the service with
`FleetApi::current(cx)`, and a test installs one over
`TokenStore::memory()` with `FleetApi::install`, which only the client's
`test-support` feature, enabled on dev-dependencies, offers.

For a one-shot read, `FleetApi::client(fleet)` hands out a ready
`FleetClient` (or the state that explains why not); run its blocking
`fleet()` and `ship(id)` in `cx.background_spawn`.

## The client

- `FleetClient::fleet()` reads `GET /api/v1/fleet` (`model::FleetOverview`)
  and `FleetClient::ship(id)` reads `GET /api/v1/ships/{id}`
  (`model::ShipDetail`). The models cover what the app shows and ignore
  unknown fields and values, so a newer Stargate, whose changes are additive,
  still reads.
- `Feed` follows `GET /api/v1/events` on its own thread: `fleet`, `ship`,
  `ship-sampled`, `host`, `activity` and `heartbeat` events update a
  `FleetSnapshot`. A dropped stream reconnects with jittered exponential
  backoff (1 s doubling to 60 s, reset after a stream stays up for 20 s).
  When Stargate refuses the stream (503 with eight streams open, or no
  stream), the feed reads the overview and every ship every 15 seconds and
  tries the stream again after two minutes. A refused token or network stops
  the feed until the token changes.
- Reads have a 10 second connect and 20 second overall timeout; the stream
  has a 40 second idle timeout, and Stargate sends a heartbeat every 15.

## Settings › Fleets

**Settings › Application › Fleets** lists each fleet with the address the
Subspace API records and its state. Paste the token and choose **Save
token**; **Test connection** reads the fleet once with the pasted token, or
the saved one, and reports in the Fleet Console's words; **Clear token**
forgets it. Saving and clearing apply at once; Settings' Save and Cancel do
not undo them. The token field is masked and emptied when a save or a clear
starts.
While the page is open every fleet is followed, so the state is the live one.

## The built-in Fleet Console

The web bridge's `subspace.fleets.get()` answers only the app's built-in
Fleet Console (board ID `fleet-console`, marked `builtIn`, not one the person
installed under that ID), and only with its `secrets` grant. It returns each
fleet as `{id, name, endpoint, token, problem}` with `token` only when it was
saved for `endpoint`; every other web Space gets an error. The page hears
`subspace:fleets-changed` when a fleet, its address or its token changes.
`subspace.fleets.get()` never reads the keychain. Only the page's
**Retry** calls `subspace.fleets.retry()`, which reads again any token the
keychain could not read (`FleetApi::reload_unreadable`) before answering,
so nothing asks to unlock the keychain without the person asking. The fleet
stays **Keychain locked** while it is read again, the page hears
`subspace:fleets-changed` only if the read finds something new, and the
service reads one fleet's token again at most once every five seconds,
whoever asks; a **Retry** asked sooner is read once, when those five seconds
end, and never again on a timer. A read still running when the person saves
or clears the fleet's token, or when the fleet leaves the initiatives, is
dropped when it lands (each of those starts a new token generation for the
fleet), so a cleared or replaced token never comes back.
See the Fleet Console's [README](../web-spaces/fleet-console/README.md#fleets).

## Fixtures

`scripts/fleet_api_test_server.py` serves a fictional fleet on loopback;
`--refuse-events` answers the stream with 503, as Stargate does with eight
streams open, and `--log-requests` prints each request with a fingerprint of
its token. `scripts/subspace_api_test_server.py --fleet` records the
aurora fleet's `fleetApiUrl`, by default the Fleet API fixture's port, or
the address `--fleet-api-url` names, which `make capture` passes with
`CAPTURE_FLEET=1 CAPTURE_FLEET_API=1`; `CAPTURE_FLEET_TOKEN` saves the
token for that address, saves none, or saves one for another address
([Fleet fixture](capture.md#fleet-fixture)).
