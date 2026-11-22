# Fleet API client

Stargate's Fleet API for the desktop, and one saved token per fleet shared by
every Space. See [Fleets and the Fleet API](../../docs/fleet-api.md) for the
design, the address rule and the API a Space uses.

- `FleetClient`: blocking reads of `GET /api/v1/fleet` and
  `GET /api/v1/ships/{id}`. Run them in `cx.background_spawn`.
- `Feed`: `GET /api/v1/events` followed on its own thread, with reconnect,
  backoff and reads on a timer while Stargate refuses the stream.
- `TokenStore`: each fleet's token in the app's vault, bound to the address it
  was saved for.
- `FleetApi::shared(cx)`: the app-wide service over the initiatives' fleets:
  their state, a ready client, and one stream per fleet however many Spaces
  follow it.
- `settings::create`: the **Settings › Fleets** page the product registers.

Requests never follow redirects or use a proxy; the token is only ever in the
`Authorization` header, never in a URL, a log line or an error.

Validate with:

```sh
mise exec -- cargo test -p subspace-client-fleet
mise exec -- cargo clippy -p subspace-client-fleet --all-targets -- -D warnings
mise exec -- cargo fmt -p subspace-client-fleet
```

The tests start `scripts/fleet_api_test_server.py` with `python3` on loopback.
