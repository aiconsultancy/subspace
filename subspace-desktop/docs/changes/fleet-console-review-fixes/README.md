# Fleet Console: review fixes

## Intent

Two reviews of the merged Fleet Console (section C of the post-merge fix
list) found a token leak, unbounded memory and several robustness gaps. In
parallel, Subspace is splitting secret writes into their own `save-secrets`
permission. This change fixes the findings and adopts that permission, with
a test for each.

## Spec

- **C1, a token stays with its address (high).** Each page fleet's secret
  holds `{v: 1, endpoint, token}` with the normalised address. Test
  connection and Save never send a saved token to a changed address: the
  form asks for the token again. On load, a token saved for another
  address, or saved without one by an earlier version, is not used; the
  map, inspector and banner say why. The Settings fleet's token is set in
  Settings with its address and is out of the page's hands.
- **`save-secrets`.** The manifest asks for it. The bridge checks it in
  `info().permissions` before calling `secrets.set` (`secrets` alone now only
  reads), and refuses the Settings `token` key outright. **Add fleet** names
  whichever of the two permissions is missing. On a Subspace without the new
  permission, adding fleets stays unavailable until it is updated; the
  Settings fleet keeps working.
- **C2, bounded event-stream memory.** Every assembled line is checked
  against 1 MiB, complete or not, and an event's data is capped at 4 MiB.
  When the parser refuses a stream, the live source cancels the response
  body and reconnects with backoff.
- **C3, capacity.** Adding a 25th page fleet is refused before any secret
  is written. The store saves the whole list or nothing and reports `full`
  or `invalid`. A failed list write puts the secret back as it was.
- **C4, WebGL2 setup failure.** A failure after the context was created
  releases the context and Canvas 2D draws on a fresh canvas, as the
  context-loss path does (`src/renderers.js`).
- **C5, the quality governor settles.** The learned display rate can now get
  slower: when stepping quality down does not speed callbacks up for two
  batches, the display sets the pace. The cooldown before retrying a level
  doubles, up to two minutes, each time that level fails soon after a retry.
- **C6, backoff.** A 200 no longer resets the reconnect backoff; a stream
  that stays up for 20 seconds does. A 200 that is not `text/event-stream`
  is a failure. A polled fleet clears its count on each complete read.
- **C7, robustness.** Requests use `redirect: "error"`. The fleet read and
  stream request time out after 15 seconds. A Retry during a background
  refresh runs once more instead of being dropped (`src/coalesce.js`). A
  slow detail read no longer overwrites a newer ship event. The WebGL2
  renderer is disposed on fallback and ignores its canvas afterwards.
  Polling pauses while the page is hidden. The issue resolver, alert
  baselines and alert watchers are bounded or pruned.
- **C8, Access docs.** The README says requests carry only the bearer
  token (no cookies, no service-token headers, no redirects), recommends
  Tailscale Serve or an SSH tunnel, and describes the one Cloudflare Tunnel
  set-up that works, with its trade-off. Credentialed requests are not
  added: a web Space cannot complete an Access login.

## Delivery

Each finding was reproduced with a failing test before the fix where the
code allowed it:

| Item | Tests | Reproduced |
| --- | --- | --- |
| C1 | `fleet-form.test.js`: Test and Save with a changed address and a blank token, same-address reuse, name-only edits, adding, loading mismatched and unbound tokens, the words shown | Yes: the saved token went to the new address and Save moved the fleet with it |
| `save-secrets` | `bridge.test.js`: writes need `save-secrets`, the `token` key is refused; `fleets.test.js`: blocker words, `canStore`; `package.test.js`: manifest | New behaviour |
| C2 | `sse.test.js`: a complete long line, an event without its blank line; `live.test.js`: a refused stream is cancelled and retried | Yes: the complete-line bypass |
| C3 | `fleet-form.test.js`: adding at capacity, rollback for adds and edits; `fleets.test.js`: all-or-nothing saves | Yes: the token was stored and the fleet silently dropped |
| C4 | `renderers.test.js`: setup failure with a fake WebGL2 context, no WebGL2, disposal on fallback | Yes: the same canvas gives no 2D context |
| C5 | `quality.test.js`: a real 30 Hz display, a GPU-bound case, a level that keeps failing | Yes: 22 level changes in three simulated minutes, ending at the lowest level; 109 changes in ten minutes for a marginal level. Now 4 and 15 |
| C6 | `connection.test.js`, `live.test.js`: 200-then-close backs off, a healthy stream resets, a non-SSE 200 fails; `fleets.test.js`: polled fleets | Yes: the attempt count reset on every 200 |
| C7 | `live.test.js`: redirects, connect timeout, stale detail; `coalesce.test.js`; `fleets.test.js`: hidden polling; `issues.test.js`, `alerts.test.js`: bounds and pruning | Yes for stale detail; the others by inspection and the new tests |
| C8 | `package.test.js`: the README and manifest match how requests are sent | Docs |

189 offline tests pass and `make package-web-space
SPACE=web-spaces/fleet-console` passes. Not run here: a browser or in-app
check (the machine is shared; the integrator schedules those).

## Tasks

- [x] C1 to C8
- [x] `save-secrets` in the manifest, bridge and Add fleet
- [x] README
- [ ] Browser and in-app check once the `save-secrets` branch is merged
