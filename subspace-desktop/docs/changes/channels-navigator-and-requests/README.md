# Channels navigator, request lines and composer hints

## Intent

The owner approved a redesign of the Channels Space; [Fleet roles in
Channels](../channels-fleet-roles/README.md) and [Agent identity in
Channels](../channels-agent-identity/README.md) built its first pieces. This
change builds the next three, from the mock-up's notes 1, 6 and 8:

- **The navigator shows the fleet** (note 1): one group per fleet with its
  Starbase, command channel and ships, a ship led by its captain's avatar
  and saying when the captain is working, and a **People** section that
  holds private conversations with people only.
- **A request line settles into a result** (note 6). Owner decision: a
  finished request keeps a settled line, **Done in 42 m** or **Failed after
  3 m**, where the line used to disappear. While it runs, the line shows the
  agent, what it is doing, a ticking clock and a small stage meter.
- **The composer teaches its keys and does real work** (note 8, in part): a
  slim hint row under the composer, and `/ask <text>` in a fleet's channel,
  which addresses the channel's agent so the request line tracks it.

Desktop only: no Framework, API or Stargate change.

## Spec

### 1. The navigator by fleet

- The roster carries each fleet on the network (`fleet/places.rs`,
  `FleetPlace`): its API id and name, its slug and the channels it claims,
  being its Starbase, command and ship channels as the roster names them and
  the channels its initiatives name. A channel two fleets on one network
  claim belongs to neither, as nothing is guessed.
- On each network: one group per fleet with a buffer open, named from the
  API and showing the network's connection state, keyed
  `irc␟<network>␟f␟<fleet id>`; then **People**, keyed `irc␟<network>␟p`;
  then the network's own group, with its unchanged key, holding the server
  row, the channels of no fleet, any older private conversation with an
  agent and the listed channels. A network that serves no fleet keeps its
  one group as before.
- A fleet's group lists its Starbase, its command channel, its ships, then
  the other channels of its initiatives, each by the name the row shows,
  which drops the `#` and the fleet's `<slug>-` prefix (`#aurora-lyra` shows
  as `lyra`). Search keeps the full name.
- A ship's row leads with its captain's small agent avatar; the Starbase and
  the command channel lead with their kind's glyph in its tone. A ship's row
  says `working` before its count while its captain has a nick status that
  the server vouches for (the heading pill's rule). Nothing else: no idle
  state is invented, and there is no crew meter, as the Space cannot read
  crew activity yet.
- Every row with unread messages is emphasised and shows its count. A
  mention adds an `@` mark before the count.
- **People** holds the private conversations with people. A private
  conversation with a nick the roster knows as an agent is never offered
  there; an older one stays readable in the network's group, where it was.
- A fleet group that is not the selected initiative's fleet starts folded
  the first time it appears. With no initiative selected, every fleet
  starts open: there is no selected fleet to set the others apart from.
- Row keys are unchanged, so selection, the stored dock layout
  (`dock/layout.rs` stores buffers) and search results restore as before.
  `Irc::group_networks` maps each group to its network for the toolbar,
  Join and the list's menus.

### 2. A request line that settles

- While a request runs, its line shows `<agent> · <what it is doing>`, the
  time since the request was sent by the server (`0:35`), and Kit's
  `Progress` as a five-step stage meter with its word. Stargate's fixed
  stages map onto it: `received`, `submitting`, `accepted`, `queued` and
  `delivered` are *accepted*; `model`, `reasoning`, `tool` and `retry`
  *working*; `waiting` and `clarified` *waiting*; `reply` *reply*;
  `completed` and `failed` *done*. The meter is named *Stage N of 5:
  <stage>*. The conversation redraws once a second only while a line runs
  and is not stale.
- Once it ends, the line settles: a quiet check and **Done in 42 m**, or a
  cross and **Failed after 3 m**, from the request's server time to the
  ending notice's, named *<agent>: Done in 42 m*. The reply count above it
  is unchanged.
- The ending's outcome and server time are kept with the ending
  (`thread_status::Ending`) and stored in its record
  (`store::Ended { outcome, at }`), so the settled line survives the line
  cap, a restart and a partial reload. A record stored before has neither
  and loads as **Done** without a duration; a held ending notice later fills
  it in once.
- The trust rule is unchanged.

### 3. The composer's hint row and `/ask`

- A slim row under the composer names its keys with Kit's `Kbd`: `@`
  mention, `/` commands, `↑` recall and `⇧⏎` new line. IRC cannot edit a
  sent message, so Up is *recall*, not *edit last*. With uploads on, the
  retention notice moves from the paperclip's row to the trailing end of
  the hint row, shortened to "Kept 30 days. Don’t paste credentials.", and
  named in full.
- In a ship's channel or a Starbase, `/` offers `/ask <text>` first,
  described as *Ask <agent> and track the answer*, then the IRC client's
  commands. `/ask` sends `<agent>: <text>` as an ordinary message
  (`Command::Say`) to the ship's captain or the rear admiral, so the request
  status line tracks it. Elsewhere, or with nothing to ask, the draft stays
  and the composer says why. `ask` names no IRC client command, which a
  test checks. The tracker commands (`/assign`, `/ready`, `/decision`) are
  later work.

### Where the design guide won over the mock-up

- No gold bar on the selected row and no accent line on the open tab:
  selection shows through the row's own fill.
- No **Jump to** field in the navigator, and no shortcut hints in the status
  bar.
- No literal colours or gradients: the mention mark and the unread count
  take theme tones (`Tone::Red`, `Tone::Accent`), not the mock-up's filled
  badges; the settled line is quiet text, not the mock-up's tinted card.
- The shared list has no caption inside a group, so **People** is a group
  of its own and the ships have no **Ships** caption (the Framework is not
  changed in this run).
- The shared completion list has no section caption, so the channel's
  commands come first and say *a command of this channel* in their
  accessible name, without the mock-up's **COMMANDS · IN #AURORA-LYRA**
  heading.
- The stage meter is Kit's `Progress`, not the mock-up's segmented bar.
- The mock-up's *delegated* and *pull request* stages and *handing NAV-2 to
  Juniper* are not in Stargate's data and are not shown.

### Decisions for the owner to confirm

- With no initiative selected, every fleet starts open; with one selected,
  every other fleet (and every fleet when the initiative has none) starts
  folded.
- `/ask` works in a ship's channel and a Starbase only; the command channel
  has no single agent, so `/ask` is refused there.
- A fleet's channel from the server's list that is not joined (such as
  `#aurora-castor` in the capture) stays greyed in the network's group with
  the other listed channels, rather than in the fleet's group.
- The mention mark uses `Tone::Red` for the `@` and keeps the count in the
  accent, as before.
- `People` is per network, named `People · <network>` when more than one
  network is configured.
- The shared composer still shows its own "Return to send · Shift+Return for
  a new line" hint while the draft is empty, so the new line key is named
  twice; removing it needs a Framework option.
- Kit's `Kbd` draws its keys with the theme's muted fill, which in Twilight
  Two is close to the panel, so the keys read as glyphs rather than caps.

## Delivery

- Navigator: `fleet.rs`, `fleet/places.rs`, `initiatives.rs`,
  `rows.rs`, `rows/sections.rs`, `rows/groups.rs`, `view.rs`, `lib.rs`,
  `conversation/header.rs` (`vouched_status`, `kind_glyph` shared).
- Request lines: `thread_status.rs` (`Outcome`, `Stage`),
  `thread_status/requests.rs` (`Ending`), `thread_status/trust.rs`,
  `thread_status/model.rs` (`Settled`), `transcript/status.rs`,
  `transcript/line.rs`, `transcript/line/message.rs`, `conversation.rs`
  (the ticker), `state.rs`, `store.rs`, `store/mapping.rs`,
  `hub/persist.rs`.
- Composer: `compose.rs`, `compose/ask.rs`, `compose/hints.rs`,
  `compose/suggest.rs`, `compose/offer.rs`, `uploads/outgoing/view.rs`,
  `fleet.rs` (`Roster::asked_in`).
- Tests: `rows/tests.rs` and `rows/tests/fleets.rs` (order, prefix
  stripping, two fleets on one network, channels of no fleet, People for
  people only, `working`, search); `tests/initiatives/navigator.rs` (the
  drawn navigator, emphasis and mention mark, folding);
  `thread_status/tests/settled.rs`, `retention.rs` and `endings.rs` (stage
  mapping, done and failed, an ending after the line cap, after a restart,
  an older record); `store/tests/endings.rs` (outcome and time stored);
  `transcript/status.rs` (clock and durations); `tests/thread_status.rs`
  (meter, settled line, the clock ticking only while live);
  `compose/ask.rs` and `compose/suggest_tests.rs` (no collision, the Say
  command, the suggestions); `tests/initiatives/ask.rs` (hint row, `/ask`
  offered and sent).
- Documentation: [Channels](../../irc.md#the-navigator-by-fleet),
  [Request status lines](../../irc.md#request-status-lines), the Channels
  guide, and the [thread status spec](../irc-thread-status/spec.md#settled-lines).

### Captures

Captured on macOS with `make capture` in Twilight Two at 1440 x 900 against a
fresh loopback Ergo 2.19.1 (`scripts/irc-test-server.sh` with
`IRC_TEST_LISTEN=127.0.0.1:16931`), restarted for each run. Every name, issue
and message is fictional.

- **Navigator and composer**: `CAPTURE_FLEET=1`,
  `scripts/irc-history-fixture.py --status --late-after 7`, a one-off
  loopback client, `nadia`, that sends the capture build one private
  message, and a Channels config naming the server with the aurora channels
  and `#lounge` and the highlight word `nav-2`. No initiative is selected,
  so Aurora starts open. Steps:
  `wait:13000;shot:…;click:75,187;wait:3000;click:600,772;type:/;wait:1500;shot:…`.
- **Request lines**: `scripts/irc-thread-status-fixture.py` and its
  `#thread-status` layout, as in the
  [thread status delivery](../irc-thread-status/delivery.md), with
  `wait:10000;shot:…;wait:42000;shot:…`.

The before images are the base pull request's own after captures, taken the
same way from this branch's base: the ship channel from
[Agent identity in Channels](../channels-agent-identity/README.md), and the
completed request from the [thread status change](../irc-thread-status/delivery.md).

## Evidence

![Before: one group per network, every channel by its full name behind a hash glyph, no hint row, and the retention notice beside the paperclip](../channels-agent-identity/evidence/after-ship-channel.png)

![After: the Aurora group with the Starbase and command glyphs, ships led by their captains' avatars, lyra working with an @ mark and 3 unread, People holding nadia, and the network's group with its server, #lounge and a listed channel](evidence/after-navigator.png)

![After: in #aurora-lyra, typing / offers /ask first, Ask aurora-lyra and track the answer, above the IRC client's commands; the hint row under the composer names @, /, ↑ and ⇧⏎ and ends with the shorter retention notice](evidence/after-ask-popover.png)

![Before: a completed request has no line left under it](../irc-thread-status/evidence/after-completed.png)

![After, running: each request shows what astra is doing, a clock since it was asked and the stage meter with its word](evidence/after-request-running.png)

![After, settled: the completed request reads Done in 1 m; a stale request keeps its last stage and the Stale pill; a queued one's clock has moved on](evidence/after-request-settled.png)

Not captured: a light theme, a narrow window, two fleets on one network
(folded or not), a failed request, and a refused `/ask`; tests cover each.

## Tasks

- [x] The navigator by fleet.
- [x] A request line that settles into a result.
- [x] The composer's hint row and `/ask`.
- [ ] Crew meter on ship rows, once the API reports crew activity.
- [ ] Captions inside a list group and a completion section heading, which
      need the Framework.
- [ ] Tracker commands (`/assign`, `/ready`, `/decision`).
