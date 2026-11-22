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
- Screenshots of Settings › Fleets are not captured yet (see Tasks).

## Tasks

- [x] `fleetApiUrl` in the API client model and the capture fixture.
- [x] `subspace-client-fleet`: models, reads, stream, feed.
- [x] Token store with the address rule.
- [x] Fleet service with one stream per fleet.
- [x] Settings › Fleets.
- [x] Fleet Console: `fleets.get` for the built-in Space and its sources.
- [ ] Capture Settings › Fleets in Twilight Two for the pull request
      (`CAPTURE_FLEET=1 CAPTURE_FLEET_API=1`, with a seeded
      `fleet-api-tokens.json`).
- [ ] Use the service in Channels (a later change).
