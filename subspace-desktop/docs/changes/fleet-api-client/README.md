# Fleet API client and shared fleet tokens

## Intent

Spaces such as Channels need each fleet's live state from Stargate's Fleet
API in native code, not only in the Fleet Console's page. The owner decided
that the desktop learns each fleet's Fleet API address from the Subspace API
and that the person saves one token per fleet on their machine, used by every
Space that needs it, Channels and the built-in Fleet Console alike. The token
is a secret: it is never stored in or sent to the Subspace API. This change
is the plumbing; wiring it into Channels is a later change.

## Spec

- **Address.** The Subspace API's fleet record gains an optional
  `fleetApiUrl` (`https`, or `http` on loopback; no credentials, query or
  fragment; no trailing slash). The desktop's model reads it as
  `Fleet::fleet_api_url`, and the capture fixture records it.
- **Client.** `clients/fleet` (`subspace-client-fleet`) reads
  `GET /api/v1/fleet` and `GET /api/v1/ships/{id}` into models that ignore
  unknown fields, and follows `GET /api/v1/events` on a background thread
  with reconnect and jittered backoff, reading on a timer while Stargate
  refuses the stream. No redirects, no proxy, bounded timeouts; the token is
  only in the `Authorization` header.
- **Tokens.** One per fleet in the app's secret vault (system keychain;
  `fleet-api-tokens.json` in debug builds), saved with the normalised address
  it was saved for and only ever handed out for that address. When the
  address the API records differs, the token is withheld and the person is
  asked to paste it again.
- **Service.** `FleetApi::shared(cx)` lists the initiatives' fleets with
  their state (not configured, no token, address changed, token refused,
  unreachable, live and the rest), hands out a ready client, and opens at
  most one event stream per fleet per process however many Spaces follow it.
- **Settings › Fleets.** Each fleet with its recorded address and state, a
  masked token field, **Save token**, **Test connection** (the Fleet
  Console's wording) and **Clear token**, applied at once.
- **Fleet Console.** The web bridge's `fleets.get` answers only the built-in
  Fleet Console, with its `secrets` grant, giving each fleet's address and its
  token only for that address; `subspace:fleets-changed` tells it to read
  again. `src/fleet-sources.js` merges these fleets between the Settings
  fleet and the page's own.

See [Fleets and the Fleet API](../../fleet-api.md) for the details and the
API a Space uses.

## Delivery

- Rust tests: the client and feed against `scripts/fleet_api_test_server.py`
  (reads, the stream, polling when the stream is refused, a refused token,
  an unreachable address) and raw loopback servers (the token only in the
  header, no redirect followed, unknown fields, a non-JSON page); the token
  store and address rule; the service (save, replace, clear, address changed,
  one shared stream, a refused token); the Settings page (save, replace,
  clear, address changed, no address); the bridge's built-in check.
- Fleet Console tests: the API fleets' shape and merge, the bridge's
  `fleets()` and its permission, and the wording for API fleets.
- Fixture script tests: `fleetApiUrl` and `--fleet-api-url`, and
  `--refuse-events`.
- Fixture script tests: `CAPTURE_FLEET_TOKEN` (`saved`, `none`, `moved`)
  and the Fleet API fixture's `--log-requests`.
- Captured from the debug capture build and verified against it (see
  [Verification](#verification)).

## Verification

The app was run in the capture harness (`make capture`, fixture HOME and
loopback servers only), in Twilight Two at 1440 x 900 with the bundled
fonts. Every capture used `CAPTURE_FLEET=1 CAPTURE_FLEET_API=1`, a first
`resize:1440,900` step and `CAPTURE_KEEP_LOG=1`; each line names its other
options and steps. The fixture's log fingerprints each request's
`Authorization` header (the first 12 hex digits of its SHA-256):
`fixture-fleet` is `f9d1c18ab98b`, the moved token `fixture-fleet-moved` is
`52bc5967f59b` and `wrong-token` is `68de4003731e`.

Running it found four faults, fixed here:

- Settings › Application had no visible Fleets row: the Application and
  Spaces trees split the column equally, so the eleventh row was clipped
  below Web Spaces, reachable only by scrolling a tree with no scroll bar.
  The Framework's navigation column now gives the Application tree its full
  height (`settings/navigation_layout_tests.rs`).
- Fleet Console, shown first, stayed in sample mode with the API fleet's
  token saved, and nothing reached the Fleet API: nothing had asked the
  initiatives catalogue for the person's fleets. `FleetApi::shared` now
  asks it (`service/tests.rs`).
- With no token, Fleet Console's sample banner said it had no Stargate
  address and pointed at its own settings. It now says what the
  initiatives' fleet needs and points at Settings › Fleets
  (`fleet-sources.js` `sampleReason`; `fleets.test.js`,
  `connection.test.js`).
- **Clear token** left a pasted, unsaved token in the field, where Save
  would store it. Clearing now empties the field (`settings/tests.rs`).

| Image | What it shows | How |
| --- | --- | --- |
| [Before](evidence/settings-navigation-before.png), [after](evidence/settings-navigation-after.png) | Settings' navigation column without and with the Fleets row (cropped). | Settings opened; before is the `feat/fleet-api-client` build. |
| [Live](evidence/settings-fleets-live.png) | Aurora with its recorded address, **Status: Live**. | `OpenSettings`, Fleets. Log: the page's event stream with `f9d1c18ab98b`. |
| [No token](evidence/settings-fleets-no-token.png) | **No token**, "Paste the fleet's Fleet API token and save it.", the token field, Save token and Test connection. | `CAPTURE_FLEET_TOKEN=none`. |
| [Saved](evidence/settings-fleets-saved.png) | After pasting and saving: **Live**, the field emptied, "Saved. Subspace sends this token only to …". | `none`, then `type:fixture-fleet` and Save token. |
| [Cleared](evidence/settings-fleets-cleared.png) | After Clear token with a token pasted: **No token**, "Token cleared from this computer." and an empty field. | `type:wrong-token`, then Clear token. |
| [Address changed](evidence/settings-fleets-address-changed.png) | **Address changed** in red, what to do, and the address the token was saved for. | `CAPTURE_FLEET_TOKEN=moved`. Log: no request at all, so none with `52bc5967f59b`. |
| [Address changed, Test](evidence/settings-fleets-address-changed-test.png) | Test connection with the saved token refuses to send it to the new address. | `moved`, Test connection. Log: no request. |
| [Test, good](evidence/settings-fleets-test-connection.png) | "Connected to “aurora”: 4 ships, Stargate 0.43.0." | Test connection with the saved token. Log: `GET /api/v1/fleet 200` with `f9d1c18ab98b`. |
| [Test, refused](evidence/settings-fleets-test-refused.png) | The masked pasted token and "Stargate refused the token (401) …". | `type:wrong-token`, Test connection. Log: `GET /api/v1/fleet 401` with `68de4003731e`. |
| [Narrow](evidence/settings-fleets-narrow.png) | The page at 900 x 600: both trees and the page scroll, nothing overlaps. | Live, then `resize:900,600`. |
| [Fleet Console, live](evidence/fleet-console-api-fleet-live.png) | The API's aurora fleet live at fleet tier. | `CAPTURE_CONFIG='{"webBoards":{"fleet-console":{}}}'` (no Settings fleet of its own), Fleet Console first. Log: the page's reads with `f9d1c18ab98b`. |
| [Fleet Console, no token](evidence/fleet-console-no-token.png) | Sample mode whose banner points at Subspace Settings › Fleets. | As above with `CAPTURE_FLEET_TOKEN=none`. Log: no request. |
| [Fleet Console, address changed](evidence/fleet-console-address-changed.png) | The banner says the address changed and to paste the token again in Settings › Fleets. | As above with `moved`. Log: no request. |

Not verified: the system keychain in a release build (capture builds keep
tokens in `fleet-api-tokens.json`), a real Stargate, more than one fleet,
and Fleet Console's own fleet beside an API fleet at another address.

## Tasks

- [x] `fleetApiUrl` in the API client model and the capture fixture.
- [x] `subspace-client-fleet`: models, reads, stream, feed.
- [x] Token store with the address rule.
- [x] Fleet service with one stream per fleet.
- [x] Settings › Fleets.
- [x] Fleet Console: `fleets.get` for the built-in Space and its sources.
- [x] Capture Settings › Fleets and Fleet Console in Twilight Two, and fix
      what running them found.
- [ ] Use the service in Channels (a later change).
