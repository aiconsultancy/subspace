# Fleet Console web Space

Fleet Console shows your Stargate fleets. A fleet is one VM running
Stargate with a rear admiral and several captains (ships), each with its
crew. It draws them with WebGL2 and drills in tier by tier, like C4 levels:

| Tier | C4 level | Shows |
| --- | --- | --- |
| Fleets (L0) | Landscape | Every fleet around this Subspace: its state, ships online, crew busy, CPU and memory. Shown with two or more fleets |
| Fleet | Context | The rear admiral, every ship and its crew in orbit, host metrics, uptimes, versions and feature flags |
| Ship | Containers | The captain, the queue, each crew member's dial, branch and linked issue |
| Crew | Components | A 24-hour ring of outcomes, one card per project, subagents, the current task, git state, pull request and issue tiles |

Click a fleet, ship or crew member to warp through it into the next tier.
Escape, the breadcrumb (`Fleets › fleet › ship › crew`) or the tier stack in
the corner pulls back out. Everything on the map is also reachable from the
inspector beside it with the keyboard. With the map focused (click it or Tab
to it), Shift+P shows or hides the frame-time readout.

## Fleets

- **Your initiatives' fleets** come from Subspace: the fleets the Subspace
  API assigns to your initiatives, each at the Fleet API address the API
  records, with the token saved in Subspace's **Settings › Application ›
  Fleets**, which every Space that shows fleets shares. Fleet Console reads
  them with `subspace.fleets.get()`, which Subspace answers only for its
  built-in Fleet Console with the **read its saved secrets** permission, and
  gives each token only for the address it was saved for. A fleet whose
  address changed since its token was saved, or that has no token, shows
  why and points to Settings › Fleets; a fleet without an address is left
  out. When no fleet has a token and Fleet Console has no Settings fleet of
  its own, the sample banner says what your initiatives' fleet needs, again
  pointing to Settings › Fleets. They are read again when Subspace sends
  `subspace:fleets-changed`. A token Subspace could not read from a locked
  keychain shows **Retry**: only Retry asks Subspace to read the keychain
  again (`subspace.fleets.retry()`), so unlock it first.
- **The first fleet** is the one in Settings (`endpoint` and `token`). With
  only that fleet, Fleet Console opens on it, as it always has, and the
  inspector offers **Add fleet**.
- **Add fleet** in the inspector takes a name, the Stargate API address
  (HTTPS, or `http://localhost` through an SSH tunnel) and its API token.
  **Test connection** reads `GET /api/v1/fleet` before you save. Each fleet's
  token is kept by Subspace with this Space's secrets (as `fleet_<id>`,
  through `subspace.secrets.set`); the list itself (id, name, address, never
  the token) is kept in the page's local storage. Each token is kept with
  the address it was saved for and is only ever sent there: **Edit** keeps
  the saved token when the token field is left blank and the address is
  unchanged, and asks for the token again when the address changes. A token
  saved for another address, or by an earlier version without its address,
  is not used until it is pasted again. **Remove** deletes the entry and its
  token. The first fleet is changed in Settings.
- Adding fleets needs a Subspace version with `secrets.set` and two
  permissions: **read its saved secrets** (to use the tokens) and **save its
  own secrets** (`save-secrets`, to keep them). Fleet Console checks both
  before storing anything; without them **Add fleet** says what to allow.
  The page never writes the Settings **API token**: `secrets.set` is used
  only for the page's own `fleet_<id>` secrets.
- **Connections.** The fleet you are inside streams `GET /api/v1/events`. The
  other fleets are read once on load and every 30 seconds while the Fleets
  tier is open and the window is not hidden. Each fleet shows its own state:
  live, reconnecting, signed out, forbidden or unreachable, and the
  inspector says what to do. Requests never follow redirects, a server that
  does not answer within 15 seconds is retried with backoff, and the backoff
  resets only after a stream has stayed up for 20 seconds.
- The tab badge counts across fleets: uncertain crew (where ship detail has
  been read), unavailable ships and fleets that cannot be read. Chat context
  names the fleet when there is more than one.
- Fleets come from one place, `src/fleet-sources.js`, which merges the
  Settings fleet, the fleets the Subspace API assigns to initiatives and the
  page's fleets, in that order, one entry per address.

It is plain HTML, CSS and JavaScript modules with no dependencies and no
build step. The scene is hand-written WebGL2 (signed-distance shapes,
instanced particles and a bloom pass), with labels on a Canvas 2D layer; web
views without WebGL2 get a Canvas 2D renderer instead.

## Performance

Fleet Console draws only while something moves, never while hidden, and
keeps its own cost low: the scene renders straight into its canvas, light for
the bloom is drawn into a small target and blurred there, large rings and the
radar sweep cover only the pixels they light, and labels redraw only when they
change (about once a second while the map idles). Ambient motion runs at 30
frames a second, and 20 while the window lacks focus or after 45 seconds
without input; warps and dispatches run at the display rate.

A governor watches frame times. When frames run slow for about a second it
steps quality down one level: the scene's internal resolution (2x, 1.5x,
1.25x, 1x; text always stays sharp), then one bloom level, then no bloom,
then 20 fps ambient motion. With sustained headroom it steps back up after
a cooldown of eight seconds, which doubles (up to two minutes) each time a
level fails again soon after it was retried. Frames are judged against the
display's rate, learned from animation-frame callbacks: a faster rate is
taken at once, and a slower one (a 30 Hz display, Low Power Mode) when
stepping quality down does not speed frames up. Nothing is saved; reduced
motion is unaffected. If WebGL2 cannot be set up, or the web view loses its
context for three seconds, Fleet Console draws with Canvas 2D on a fresh
canvas.

The frame-time readout shows median and p95 frame time, CPU time, GPU time
where the engine exposes timer queries (Chromium does, WebKit does not), the
frame rate, label redraws and the quality level. Inside Subspace, set
**Frame-time readout** to **On** in Settings › Spaces › Fleet Console, or
press Shift+P with the map focused; Shift+P flips it until the setting next
changes. In a browser, `?perf=1` also turns it on.

## What it reads

Fleet Console reads Stargate's read-only Fleet API v1 with a bearer token:

| Route | Used for |
| --- | --- |
| `GET /api/v1/fleet` | The first view of a fleet; every 30 s for the fleets you are not inside while the Fleets tier is open; **Test connection** |
| `GET /api/v1/events` | Live updates: `fleet`, `ship`, `host`, `activity` and `heartbeat` events; with `?agent=<id>` while an Activity view is open, also `agent-activity` |
| `GET /api/v1/ships/{id}` | Fresh detail when you warp into a ship |
| `GET /api/v1/ships/{id}/crew/{member}` | Fresh detail when you warp into a crew member |
| `GET /api/v1/agents/{id}/activity` | An officer's recent turns when its Activity view opens, and again whenever the view must catch up |
| `GET /api/v1/agents/{id}/activity?turn=&item=&page=` | The pages of a large item, read when you ask for it |
| `GET /api/v1/ships/{id}/crew/{member}/diff?project=&mode=&cursor=` | A crew member's changed files for one project when its diff view opens, page by page |
| `GET /api/v1/ships/{id}/crew/{member}/diff/file?project=&mode=&path=&page=&at=&side=` | The next page of a long patch, or a page of a binary file's bytes, when you ask for it |

The event stream is read as a streamed `fetch` body, because `EventSource`
cannot send an `Authorization` header. If it drops, Fleet Console reconnects
with jittered backoff and resumes from the last event ID; three missed
heartbeats count as a dropped stream. The toolbar and a banner always say
whether it is live, reconnecting, signed out (401), forbidden (403) or
unreachable, and what to do about it.

## Set up Stargate

1. **Enable the API.** Add an `api` block to the fleet VM's Stargate
   configuration and restart Stargate. It listens only on loopback or a
   private address, never a public one:

   ```json
   { "api": { "listen": "127.0.0.1:8484" } }
   ```

   Its `allowedNetworks` defaults to private ranges, and its `corsOrigins`
   default (`subspace://*` and `https://subspace.*`) already allows web
   Spaces on macOS and Windows. See Stargate's API documentation for every
   option.

2. **Get the token.** On the fleet VM, run `stargate api token`. It prints the
   token (creating it the first time). `stargate api token --rotate` replaces
   it; restart Stargate afterwards.

3. **Reach it privately.** Pick one:

   | Route | Endpoint to enter | Notes |
   | --- | --- | --- |
   | Tailscale Serve | `https://<vm>.<tailnet>.ts.net` | `tailscale serve --bg 8484` on the VM serves the loopback listener over HTTPS to your tailnet. Recommended |
   | SSH tunnel | `http://localhost:8484` | `ssh -N -L 8484:127.0.0.1:8484 <vm>`; plain HTTP is allowed only to localhost |
   | Cloudflare Tunnel | `https://fleet.example.com` | Only with the Access set-up described below |

   Web Spaces may connect only over HTTPS or to loopback HTTP, so an address
   such as `http://100.101.102.103:8484` cannot be used directly.

   Fleet Console sends the Stargate bearer token and nothing else: no
   cookies (requests use `credentials: "omit"`), no Cloudflare Access
   service-token headers, and it does not follow redirects. So a Cloudflare
   Access policy that needs a browser login or a service token stops every
   request before it reaches Stargate. A Cloudflare Tunnel works when Access
   lets the API through on the bearer token alone: leave `/api/v1/*` out of
   the Access application or give that path a Bypass policy, and, if Access
   still covers the hostname, turn on its option to pass `OPTIONS` requests
   to the origin so CORS preflights reach Stargate. That puts the Fleet API
   on the internet behind its token (through the tunnel Stargate sees the
   connector's local address, so `allowedNetworks` does not narrow it), which
   is why Tailscale or an SSH tunnel is the better route.

## In Subspace

Fleet Console is built into the Subspace App on macOS and Windows (Linux does
not show web Spaces yet). The build embeds this folder's package files, the
files `make package-web-space` puts in a ZIP by its default rules, and fails,
as that script does, on a link, a special file or a name that is not portable;
it does not apply the script's size limits. At each start the app
writes them to `web-board-apps/fleet-console` beside its config file and keeps
a `webBoards.fleet-console` entry pointing at them, marked `"builtIn": true`.
It is in the tab strip from the first start, in sample mode until it has an
address and a token. Being part of the app, it starts with every permission
its manifest asks for allowed.

Open **Settings > Spaces > Fleet Console** and:

1. Enter the **Stargate API address** and the **API token**. `./install.sh`
   writes both for the fleet it installs when the desktop is on the same
   machine.
2. Check the permissions: **read its settings** and **read its saved
   secrets** (the token) are needed for a live fleet; without either it shows
   the sample fleets, unless you add fleets in the page. **Save its own
   secrets** lets you add more fleets in the page; **read and open issues**
   gives issue tiles and badges, from the board that answers the issues
   contract; **show a badge on its tab** counts uncertain crew plus
   unavailable ships; **share context with chat** shares the selected crew
   member's ship, task, phase, branch, pull request and issue identifiers,
   never the address or token; **show notifications** tells you when a crew
   task becomes uncertain or a ship becomes unavailable, at most one every
   five seconds and never for what was already true when it opened.
3. Save.

Turn it off in Settings to hide it. Removing it under **Settings >
Application > Web Spaces** only resets it: the app adds it again at the next
start, with its permissions allowed. An entry you install yourself under the
ID `fleet-console` (from a folder, a ZIP or GitHub) replaces the built-in one
and is left alone. To try a changed copy of this folder without rebuilding,
add it as a **Local folder** web Space under another ID; it then starts with
every permission off, like any installed web Space.

**Motion** set to **Reduced**, or the system's reduce-motion setting,
replaces the warp with a short crossfade and stops orbits, the radar sweep
and blinking. **Frame-time readout** (Off by default) shows frame times over
the map; see Performance.

## Officer activity

**Activity** in the rear admiral's and each captain's inspector section
opens what that officer is doing: its recent turns, newest first, with the
running turn expanded. Each turn shows its steps (model, settled tokens and
cost), tool calls (name, full input, progress as it runs, full output and
errors), reply text and reasoning text, updating live. **Back** or Escape
closes it, as does moving to another tier; focus returns to the button.

- **Nothing is cut.** Content is shown in full. Long text wraps, scrolls
  inside its block and stays selectable. An item larger than 64 KiB arrives
  paged: **Load in full** reads every page, one page at a time, and shows
  it whole; if the item changes between pages the read starts again from
  page 0, up to five times, and a page that is not the one asked for or
  does not add up to the item's size stops the read with an error and
  **Try again**. An item too large for the view (over 64 MiB, or more than
  its budget) is read and shown a part (256 KiB) at a time, with First
  part, Previous part, Next part and Last part, so every byte is still
  reachable. Text longer than 64 KiB or 1,000 lines shows one section at a
  time with First, Previous, Next and Last; a closed turn holds no items,
  and an open one shows 20 at a time with **Earlier items** and **Later
  items**. The console has one 64 MiB memory budget, shared by the
  Activity and crew diff views; the latest request of either is never
  freed for the other. It is a cache budget
  only: nothing is hidden or cut to keep within it. The snapshot's own
  content is charged first and is never freed. Items read whole are charged
  twice (the item and the copy drawn from it), plus, after **Show**, what
  their section costs beyond that; a part is charged with the one section
  it draws, at most 64 KiB as shown. What is drawn is charged as it shows,
  markers included. An item read whole must fit beside the rest with both
  copies; otherwise it is read in parts. One already on show that stops
  fitting (the snapshot grew, **Show** needs more, or its newest version is
  too large) is read in parts from the part holding what it shows, and its
  section stays on show until that part arrives. **Load the latest** reads
  the newest version from its first part instead, also when the snapshot
  grows while it reads; closing the turn meanwhile lets go of the section
  but not of the read. Past the budget, the least recently opened item
  read is freed and offers **Load in full** again; this frees only the
  view's copy, never the item on the gateway.
  A body with no room left shows "Not drawn, to stay within the view's
  memory budget" with its size and **Show**; its turn stays open.
  Bodies are drawn in this order: those asked for with **Show** (the latest
  first), then those of turns opened by hand (the turn opened last first),
  then those of the running turn, which opens by default. The latest thing
  you asked for (a **Show**, a **Load** or another part) is never freed for
  anything else; a later request ends that claim, so an earlier **Show**
  goes back to its caption. A body still larger than the room draws one
  section at a time. The view can go past 64 MiB only by its floor: the
  snapshot's content, plus one incoming part (at most 256 KiB) and one
  drawn section (at most 64 KiB), which, while the part is read, is the
  one it replaces. A loaded item is not read again
  by itself when it changes, as a running tool changes often: it says a
  newer version is on the gateway and offers **Load the latest**; its body
  stays as it is, so a selection in it survives. While the latest is read,
  even in parts, the body on show stays, charged as before beside the
  incoming part, and stays with the error and **Try again** if the read
  fails. Items read belong to the stream's generation: when it changes,
  they are dropped and their reads stop; a snapshot read again in the same
  generation keeps them. When a tool call has both parsed and streamed
  input, both show. Fields an item, its error or an output
  part carries beyond those laid out show as **Other fields**, beside a
  text part's text in a long one's sections too. An item or content of a
  kind this console doesn't know, or that fails its checks, shows as JSON.
- **Content is text.** Every value is set as text, never parsed as HTML.
  Control characters (other than newline and tab) and bidi formatting
  characters show as `⟨U+XXXX⟩` markers, in JSON keys and values too, so
  content cannot hide or reorder itself; a copy is exactly what shows.
  Sizes and sections count the text as it shows: each marker is 12 bytes,
  and no section splits one. A
  turn or item the console cannot read shows as the JSON it came as. Activity stays in memory only: it
  is never stored or logged, never reaches chat or an agent's context, and
  closing the view drops it. A change to the fleet's address or token drops
  it too, and the view starts again from the new stream; removing the token
  or the fleet closes the view.
- **One stream.** The gateway allows eight event-stream viewers in total, so
  the view never opens a second stream: the fleet's stream re-opens with
  `?agent=<registration ID>` while the view is open and without it after.
  The re-open is quiet (no Connecting pill) and resumes from the last event
  ID. A gateway that refuses the parameter (400 or 404, or a ninth agent)
  gets its plain stream back, and the view reads the snapshot every five
  seconds instead, saying so.
- **Joining.** The snapshot is read only after the stream has answered, as
  the gateway registers the watch when the events response starts. Events
  that arrive meanwhile are buffered, up to 512 events or 8 MiB (past
  that they are dropped and the snapshot is read again); those at or below
  the snapshot's
  cursor are dropped. A `seq` that fails to rise, a new `generation`, an
  item for an unknown turn and every stream re-open read the snapshot
  again, one read at a time. A skipped `seq` is not a gap. A turn the view
  no longer holds is placed by its start time, so a late end for a turn
  already dropped never shows as the newest. If that read
  fails, the view keeps the last activity read, marked stale with the
  error, holds changes rather than applying them and reads again by itself
  after 1, 2, 4 … up to 30 seconds (or the gateway's `Retry-After`, up to
  30), then applies the held changes newer than what it read. **Try again**
  is offered throughout and waits for the current stream to answer.
- **Messages.** "Some changes may be missing (the stream reconnected)" on a
  turn the gateway saw its officer's stream reconnect in; "This officer's
  backend isn't collected yet." when the gateway does not collect it; and
  "Activity isn't available on this gateway." on a 404.

The officer is identified by its registration ID: `fleet.admiral.id` for the
rear admiral and `ship.captain.id` for a captain.

## Crew diffs

**View diff** on a project in a crew member's inspector card opens what that
member has changed in its checkout: **All changes**, **Committed only** or
**Uncommitted only**. It shows only when the gateway lists the
`api.crewDiffs` feature as on and the project's Git state was read without
an error. **Back** or Escape closes it, as does moving to another tier.

- **Nothing is cut.** The whole file list is read before it shows, 200
  files at a time on screen. A long patch shows one part (a gateway page)
  at a time with **First part**, **Previous part**, **Next part** and, once
  the count is known, **Last part**, each part 64 KiB or 1,000 lines on
  screen at a time; a failed page keeps the part on show and offers **Retry
  page**. A binary file's **Old bytes** and **New bytes** open a hex and
  ASCII view read a part (a page of up to 1 MiB) at a time, 4 KiB on
  screen, moving across parts as you page. Parts count against the
  console's 64 MiB budget; one freed to make room is read again, at the
  same snapshot, when shown. Images also offer a preview of up to 10 MiB
  and 4,096 pixels a side, which takes only free room and gives it back
  first; one that does not fit says so with its size. Long lines wrap.
- **Content is text.** Every value is set as text, never parsed as HTML,
  and control and bidi formatting characters show as `⟨U+XXXX⟩` markers.
  Diffs stay in memory only, never reach chat, and closing the view drops
  them.
- **One snapshot.** Every page is read at the snapshot's fingerprint. When
  the checkout moves on (the `ship` event's fingerprint changes, or the
  gateway answers 409), the view says "Changed since loaded · Refresh" and
  stops reading; a snapshot the gateway saw change while reading says
  "Changed while loading · Refresh". Refresh, a mode change and closing the
  view cancel what is still being read, including a read waiting to retry
  while the gateway is busy.
- **Messages.** "Diff not available" on a 404, as from a gateway older than
  crew diffs or a member with no checkout; "No changes." for an empty
  snapshot.

## Sample mode

With no Subspace bridge (a plain browser), no address, or missing
permissions, Fleet Console shows clearly labelled sample fleets: two
simulated fleets that keep changing and one that never answers, so the
Fleets tier, the warp and the unavailable state are visible without setup.
The samples use the same data shapes and event path as live fleets. The
banner says what is missing. Each sample officer's **Activity** shows
invented turns: a long, paged test log, a failed tool call whose output
holds markup, a terminal escape and a bidi override, and a running turn that
streams its tool progress and reply, then ends and starts another. Officers
on a backend other than OpenCode, such as the sample rear admiral, show the
not-collected message. Each sample crew project's **View diff** shows an
invented checkout: a patch in three pages, a rename, an untracked note with
a bidi override and a long line, a PNG, a binary larger than one page with
an empty old side, a submodule and a symbolic link.

To look at it in a browser, serve this folder and open
`http://127.0.0.1:8765/`:

```sh
python3 -m http.server --bind 127.0.0.1 8765
```

Preview parameters, for checking the design: `?theme=light` or `?theme=dark`
forces a palette, `?motion=reduced` reduces motion, `?renderer=2d` uses the
Canvas 2D renderer and `?slowmo=20` runs animations twenty times slower.
`?perf=1` shows the frame-time readout from the start (Shift+P still hides
it); `?quality=N` pins level N (0 is the full look) and turns the governor
off.

## Themes

Inside Subspace, Fleet Console follows the app theme through
`subspace.theme()` and the `subspace:theme` event: tones become CSS custom
properties and shader colours, and the theme's UI and mono fonts are used.
Dark themes add bloom light; light themes tint toward the glow colour
instead, so nothing blows out to white. In a browser it uses Subspace One
Dark, or Subspace Channel Light when the system prefers light.

## Development

Run the offline tests with the pinned Node.js through mise. From this folder:

```sh
mise run test
```

From `subspace-desktop/`, `mise run fleet-console-space-test` runs the same
tests; it is part of `make check`. `test/fixture-fleet.test.js` also reads the
desktop's Stargate Fleet API fixture (`scripts/fleet_api_test_server.py`, the
fictional `aurora` fleet that `CAPTURE_FLEET_API=1 make capture` serves on
loopback) through the contract, reducer, model and inspector. They cover the SSE parser (chunk
boundaries, multi-line data, comments, heartbeats), the reducer, the live
source and its reconnects, the connection state machine, issue identifier
extraction and batching, layouts and the warp camera, formatters, sample data
against the contract, alerts, the inspector's escaping, theme mapping, the
quality governor, label dirty tracking, render batching, the fleet store
and its failure modes, address validation, the source merge, per-fleet
connections and poll timing, the Fleets layout, navigation and breadcrumb
through L0, officer activity (decoding, page joins, the snapshot and event
join, refetches, join order, the one stream's `agent` parameters, paged
and part reads within a bounded budget, bounded rendering that never parses
HTML and marks hidden characters), the pager, crew diffs (the feature gate,
status codes, cursor, patch and binary parts, the shared budget, the image
preview's room, the fingerprint banner,
cancellation and source changes), and the package rules.

| Path | Purpose |
| --- | --- |
| `subspace-board.json`, `index.html`, `styles.css` | Manifest, page and styles |
| `src/main.js` | Page wiring: mode, frame loop, navigation, inspector, bridge effects |
| `src/api.js`, `src/live.js`, `src/sse.js`, `src/connection.js` | Fleet API client, event stream, SSE parser, connection states |
| `src/activity.js`, `src/activity-view.js`, `src/activity-controller.js` | Officer activity: snapshot and event join, paged items; the Activity view; its lifecycle in the inspector |
| `src/pager.js`, `src/budget.js` | Reading and showing a part at a time, fenced by revision; the console's byte budget, least recently opened first, shared by views through scopes |
| `src/diff.js`, `src/diff-parts.js`, `src/diff-view.js`, `src/diff-controller.js`, `src/sample-diff.js` | Crew diffs: paged list, patch and binary parts; their part sources and patch lines; the diff view; its lifecycle in the inspector; the sample checkout |
| `src/contract.js`, `src/reducer.js`, `src/model.js` | Payload checks, state reducer, derived views |
| `src/sample.js`, `src/sample-source.js` | Sample fleets and their simulation |
| `src/fleet-sources.js`, `src/fleet-store.js`, `src/fleet-manager.js` | Fleet sources and their merge, the page's fleet list, one connection per fleet |
| `src/coalesce.js` | One settings refresh at a time, keeping a Retry asked for meanwhile |
| `src/fleet-panel.js`, `src/fleet-form.js`, `src/scene/landscape.js` | Fleets panels and the Add / Edit form, the Fleets tier |
| `src/bridge.js`, `src/issues.js`, `src/alerts.js`, `src/effects.js` | Bridge wrapper, issue references, notifications, chat context |
| `src/layout.js`, `src/camera.js`, `src/stars.js`, `src/geometry.js` | Tier layouts, navigation and warp camera, star field, point lists |
| `src/scene/` | Builds each tier as a draw list both renderers read |
| `src/gl/` | WebGL2 renderer, instance batching, shaders and helpers |
| `src/canvas2d.js`, `src/overlay.js` | Canvas 2D fallback renderer, text overlay with dirty tracking |
| `src/renderers.js` | Choosing WebGL2 or Canvas 2D, and switching to Canvas 2D on a fresh canvas when WebGL2 fails |
| `src/quality.js`, `src/perf.js` | Quality levels and governor, frame statistics, the frame-time readout and its Shift+P toggle |
| `src/inspector.js`, `src/html.js`, `src/chrome.js`, `src/theme.js` | Inspector panels, escaping, window chrome, theme tones |
