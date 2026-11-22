# Fleet Console built in

## Intent

Fleet Console was a web Space users installed by hand under **Settings >
Application > Web Spaces**, so a fresh Subspace, and every capture, had no
Fleet Console tab. The owner wants it in the app: in the tab strip of default
and release builds with nothing to install, and shown live against a fleet
that looks real in captures, not in its sample mode. There are no users or
installs yet, so nothing is migrated.

## Spec

- **Built in.** The product's build script lists the package files of
  `web-spaces/fleet-console` with the package script's rules (no tests, no
  `mise.toml`) and embeds them (`products/subspace/build/web_spaces.rs`). At
  a desktop start on macOS and Windows, before the config file is read,
  `products/subspace/src/web_spaces.rs` calls
  `subspace_web_board::builtin::install`, which writes the files to
  `web-board-apps/fleet-console` beside the config file when they differ and
  keeps a `webBoards.fleet-console` entry pointing at them, marked
  `"builtIn": true`. From there it is an ordinary folder web board: the
  Space list, the order, Settings, grants and secrets work as for any other.
  Command-line runs leave the config file alone. Linux shows no web Spaces,
  so Linux builds add nothing.
- **No new feature or guard.** Fleet Console ships, so it is not behind a
  Cargo feature and the release guard for unreleased Spaces is untouched.
- **Whose entry.** A new entry, or one with neither `url` nor `path` (as
  `install.sh` and `make capture` write to set the address and token before
  the app starts), becomes the built-in one, with every permission the
  manifest asks for allowed: it is the app's own code. A built-in entry only
  gets its `path` refreshed; grants the user turned off stay off. An entry
  with a `url`, a `managedSource` or a `path` of its own is the user's and is
  left alone. Turning the Space off hides it; removing it resets it at the
  next start.
- **Fleet API fixture.** `scripts/fleet_api_test_server.py` serves Stargate's
  Fleet API v1 on loopback (`GET /api/v1/fleet`, ship and crew detail and the
  event stream, with Stargate's CORS rule for `subspace://` origins) for the
  demo world's `aurora` fleet: the rear admiral, four ships with sixteen crew,
  nine busy, five tasks queued, one uncertain, host and gateway metrics,
  feature flags, IRC request progress, branches, pull requests, subagents and
  outcomes. A small simulation moves the crew through phases. Crew work names
  `NAV`, `MAP`, `WARP` and `RELAY` issues; the Subspace API fixture's
  `--fleet` option now adds those projects and issues beside `FLT`.
  `CAPTURE_FLEET_API=1` starts it and points Fleet Console at it
  ([Fleet fixture](../../capture.md#fleet-fixture)).
- **Populated demo data.** For the README's screenshots, the Subspace API
  fixture's `--fleet` option fills `NAV`, `MAP`, `WARP` and `RELAY` like a
  busy team's tracker (58 issues in every state, trackers and sub-issues,
  labels, priorities, due dates, people and captains as assignees, linked pull
  requests, comments, and 13 decisions in every status with supersession
  chains), and the GitHub fixture gains the same work's pull requests and a
  Release workflow with runs in every state, including re-run attempts.
- **Actions on by default.** The Actions Space was off by default in every
  build; the owner wants it in the tab strip, so its descriptor default is
  now on.

## Delivery

- Tests: `spaces/web/src/builtin/tests.rs` (new, adopted, user-owned and
  moved entries, revoked grants, replaced files, unreadable config, unsafe
  names), `products/subspace/src/web_spaces.rs` (embedded files and package
  rules), `scripts/test-fleet-api-test-server.py` (routes, token, CORS,
  stream, simulation, issue references against the Subspace API fixture),
  `scripts/test-capture.sh` (`CAPTURE_FLEET_API`) and
  `web-spaces/fleet-console/test/fixture-fleet.test.js` (the fixture through
  the console's contract, reducer, model and inspector).
- Screenshots, in Twilight Two at 1440 x 900, in the repository's
  `docs/assets/screenshots/`: Channels with a live issue tile, Issues, an
  issue page, Pull requests and a pull request page, Actions and a run page,
  Decisions, and Fleet Console at fleet, ship and crew tier and mid-warp. All
  show Fleet Console and Actions in the tab strip. Channels used
  `scripts/irc-test-server.sh` with `scripts/irc-history-fixture.py`.

## Tasks

- [x] Embed Fleet Console and add it at start.
- [x] Fleet API fixture, capture option and tests.
- [x] Docs, `install.sh` and `INSTALL.md`.
- [ ] `install.ps1` still tells Windows users to install the Space by hand
      when its entry is missing; it fills in the address and token once the
      app has started.
