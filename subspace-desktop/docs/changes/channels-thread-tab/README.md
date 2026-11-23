# Channels Thread tab

## Intent

The owner's mock-up of the Channels redesign gives a ship's channel's
inspector the tabs **Ship · Thread · People** (a Starbase **Starbase ·
Thread · People**) and a settled request line reading `Done in 42 m · Wren
closed NAV-6 · 3 replies ›`. [Channels fleet live](../channels-fleet-live/README.md)
built Ship, Starbase and People. This change builds **Thread**: one thread at
a time, being a request (a message addressed to an agent) or any message
with replies, opened from its reply count or its status line, with a way to
reply into it. The mock-up does not draw the tab's contents, so they are
designed here within the design guide, kept plain.

Desktop only: no Framework, API or Stargate change.

## Spec

### What the tab shows

Top to bottom (`view/inspector/thread.rs`, `thread/steps.rs`):

1. **The root message**: author, rank pill, time and text with its reference
   chips, drawn by the transcript's own header and body
   (`transcript::thread_message`), without avatar, tiles or attachments.
2. **Status**, for a request with trusted progress or a kept ending: a plain
   list, one row per step the agent reported. A step is a change of the
   status line's stage (`thread_status::Stage`): **Accepted**, **Working**,
   **Waiting for an answer**, **Writing the reply**, then **Done**,
   **Failed** (red text) or **Ended**. Each row has its server time and how
   long it lasted, until the next step began; the ending has the request's
   whole time (`in 42 m`, `after 3 m`); a running last step says
   `since 17:50` and no duration. If the ending is kept but no notice before
   it is held: `The steps before the end are not loaded.`, with **Load around
   this message** (`Command::LoadAround`, as Search uses) while the network
   is connected and keeps `draft/chathistory`.
3. **Replies · N**, grouped by author by the transcript's rule
   (`lines::continues`), or `No replies yet.`

With no thread chosen the tab says, in one line, how to open one. Its footer
holds **Reply in thread**, or, without a connection or `message-tags`, the
reason in its place.

### Which messages belong, and the trust rule

- A reply is a message (`PRIVMSG`, action or notice), not folded progress,
  whose `+draft/reply` leads through held parents to the root, read by the
  same `ThreadRoots` as the transcript's reply counts
  (`thread/model.rs`). The tab's replies and the count therefore agree, a
  reply to a reply belongs, and nothing in a message's text places it in a
  thread.
- A step is only a notice the buffer folded (`Buffer::folded`), which it does
  only when the notice's server-set `account` equals the nick the held
  request asks: the rule the status line already uses. An untrusted or
  lookalike notice stays an ordinary reply and never makes a step; a notice
  after the ending makes none.
- A message addressed by its first word but with no progress, such as
  `Morning, all.`, has no timeline, as it has no status line.

### Opening it

- In a ship's channel or a Starbase (`thread::has_tab`) a message's reply
  count reads `3 replies ›` and is a button; a request's running or settled
  line is a button and ends with `· 3 replies ›` only when it has real
  replies, and the separate count row is then left out. Choosing either
  emits `ConversationEvent::OpenThread(<msgid>)`; the board chooses the
  thread, shows the inspector if hidden and opens its Thread tab. Enter on a
  focused message with either does the same (`conversation/keys.rs`).
- Elsewhere (the command channel, other channels, private conversations)
  counts and lines stay read-only as before: there is no Thread tab there.

### State

`thread::Threads` keeps one chosen root per conversation for the session,
so it survives the inspector's tabs and switching conversations. On each
board sync it rebuilds a chosen thread only when its buffer's fingerprint
changed (held count, first and last sequence, folded count, the root's
ending), so a sync that leaves the buffer alone reads none of its lines; it
forgets a thread whose root is no longer held (line cap, conversation
removed) and takes its composer out of reply mode.

### Reply mode

**Reply in thread** puts the conversation's composer into reply mode
(`compose/reply.rs`): a row above the input, `Replying in thread to mira:
<start of the root>`, with **Cancel reply** (×), Escape also cancelling, and
the placeholder `Reply in thread to mira`. Sending sends
`Command::Reply { buffer, msgid: <root>, text }`: a `PRIVMSG` to the channel
with `+draft/reply=<root msgid>`, so everyone in the channel sees it, then
leaves reply mode. A `/command` is refused (a leading `//` is a literal
slash), as are attachments, each with the reason in the composer. Reply mode
starts only in a channel; `reply::command` refuses any other buffer, so
nothing here sends a private message. Without a connection or `message-tags`
it is refused with the same reason the footer shows. Choosing a different
thread leaves reply mode.

### Design choices where the mock-up is silent

- No avatars in the tab: the root and replies show author, rank pill and
  time as text, so no agent icon appears and the Icons setting has nothing
  to govern here.
- The timeline is a plain list without dots or a rule: the design guide has
  no timeline pattern. Selection, grouping and the failed step show through
  text and tone; there is no accent edge on the tab, the rows, the root or
  the reply row.
- The tab's content starts at the inspector body's own text edge, with no
  inset of its own. So do Ship and Starbase, which added the text edge and
  a top padding of their own until this change, so every Channels
  inspector tab (Ship, Starbase, Thread, People) now starts at the same
  edge and top: the design guide's inspector body edge,
  `workspace::text_edge`.
- The tab has no count: the reply count is in the Replies heading.

### Left out, and what each needs

- **`Wren closed NAV-6`** on the settled line, and the mock-up's
  *delegated* and *pull request* steps: Stargate's `+stargate/progress`
  carries a stage only. They need the progress contract to carry who closed
  what, delegation and a pull request.
- **A reply count from the server**: counts and the tab cover loaded lines
  only; this needs the reply-search follow-up (a server lookup by thread).
- **Attachments in a reply**: the upload path sends its own lines without
  the reply tag; it needs `uploads::outgoing` to carry a reply target.
- **A reply in a private conversation**: by the owner's rule, never.

## Delivery

- Model and state: `spaces/irc/src/thread.rs`, `thread/model.rs`,
  `thread/chosen.rs`; `thread_status` exposes `agent_name`.
- Transcript: `transcript/replies.rs` (`ThreadLink`, `link`, the count as a
  button), `transcript/status.rs` (the line as a button with the count),
  `transcript/line/message.rs` (`thread_message`, `thread_facts`),
  `RowContext::threads`.
- Inspector: `view/inspector.rs` (`InspectorTab::Thread`, three tabs, the
  footer), `view/inspector/thread.rs`, `thread/steps.rs`, `thread/footer.rs`.
- Composer: `compose/reply.rs`, `compose.rs`, `compose/send.rs`.
- Board: `conversation.rs` (`OpenThread`), `dock/open.rs`, `view.rs` (the
  refresh on sync), `conversation/keys.rs` (Enter).
- Fixture: `scripts/irc-history-fixture.py --thread` and
  [Desktop capture](../../capture.md).
- Tests: `thread/tests.rs`, `thread/tests/timeline.rs` and
  `thread/tests/lifetime.rs` (membership, nested replies, foreign buffers,
  folded progress, steps and durations, a running step, untrusted notices,
  an ending without its notices, the count agreeing, where the tab is
  offered, reply refusal, rebuild only on change, lifetime under the line
  cap and on removal); `compose/reply/tests.rs` (what is sent, to where,
  with which tag, the literal slash, refusals); `view/inspector/thread/steps.rs`
  (step words); `view/inspector/tests.rs` (three tabs);
  `tests/initiatives/thread.rs` (render tests: the prompt, the settled line's
  count and click, the root, steps and replies drawn, a count opening a
  plain message's thread and a reply arriving live, the choice kept per
  conversation, reply mode sending `Command::Reply` and cancelling, the
  footer's refusal without `message-tags`).
- One inspector edge: `view/inspector.rs` (`subject_body` adds no inset);
  `tests/initiatives/views.rs`
  (`every_inspector_tab_starts_its_content_at_the_body_s_text_edge`).
- Documentation: [Channels](../../irc.md#the-thread-tab) and the in-app
  Channels help.

## Findings carried from pull request 69

The review of [Channels fleet live](../channels-fleet-live/README.md)
left thirteen findings open, none critical or high, to fix here. Each was
checked against the code after merging `main` (pull request 69 with its
review fixes) into this branch. Every fix here has a test. Those of 1 and 12 were run against
the code before the fix and failed there; 2 and 3 moved to their own pull
request; the Channels tests (4 to 9 and
11) were written to fail on it by what they check, but were not run
against it.

1. **An in-flight keychain retry could bring back a cleared or replaced
   token** (`clients/fleet/src/service.rs`). Real. Fixed in `c763063`
   (`fix(fleet): never bring back a cleared or replaced token from a read
   still running`; its fields grouped in `efff5a6`): a save, a clear or the fleet's removal starts a new
   token generation for the fleet, and a read or save that began in an
   older one is dropped when it lands. A cleared token stays cleared.
   Tests: `service/tests/keychain.rs`,
   `a_retry_still_reading_never_brings_back_a_cleared_token` and
   `…_a_replaced_token`, each over 40 schedules; both failed before.
2. **A listed pull request kept its summary after GitHub refused the read** (Sol). Moved, with its fix and tests, to its own
   pull request on `fix/github-reference-access`; not resolved here.
3. **A read started after a refresh's first section could override a later
   section's fresher list** (Sol). Moved likewise; not resolved here.

4. **Each Channels view replaced the process-wide fleet map**
   (`spaces/irc/src/live/watch.rs`). Real: the hub is one per process.
   Fixed in `891b7d2` (`fix(irc): keep each view's fleets and say when the
   fleet's data is not current`): each view publishes its own share
   (`Hub::fleet_views`) and `Hub::fleets` is their union; a closed view's
   share goes with it (its release hook; the render test covers a view
   leaving the front, not the release itself). Test:
   `a_hidden_view_on_the_same_hub_never_empties_the_fleets_a_shown_one_shows`
   and `the_hub_shows_every_view_s_fleets_and_a_hidden_view_takes_none_away`.
5. **Channels could start the fleet service before showing any fleet**
   (`live/watch.rs`). Real. Fixed in `891b7d2`: the shown fleets are
   worked out first and the service is reached only when there is one.
   Test: `a_view_that_shows_no_fleet_never_starts_the_fleet_service`.
6. **The Ship tab said "Not listed" while the first overview was pending
   or kept** (`view/inspector/ship.rs`). Real. Fixed with 11 in `891b7d2`.
7. **Fold inheritance ran only on a fleet group's first appearance**
   (`view/navigator.rs`). Real. Fixed in `ef948ee` (`fix(irc): fold a fleet
   group as its network's when it begins to take in the network's rows`). Tests:
   `a_folded_fleet_that_takes_in_the_network_s_rows_opens_as_the_network_s_group_was`
   and the pure `fold_to` rule.
8. **An issue scope change could give an issue and a kept pull request
   one place in a message** (`tiles.rs`). Real. Fixed in `93662d0`
   (`fix(irc): never show a bare key in play, and give each claim its own
   place`): a message
   read again gives its own claims their place among what it names now.
   Test: `a_new_recognition_never_gives_an_issue_and_a_kept_pull_request_one_place`.
9. **The Ship inspector sorted and cloned every tile reference each
   render** (`view/inspector/play.rs`). Real, minor. Fixed in the same
   commit: `in_play_first` selects the first eight before any is cloned or
   the rest sorted. Covered by the same test.
10. **The design guide's checklist addresses reviewers**
    (`subspace-framework/docs/design-guide.md`). Left as it is, by
    decision: it is the repository's own UI review checklist.
11. **The Ship tab misstated why it showed nothing while the fleet's data
    was kept** (`ship.rs` with `live/ship.rs`). Real. Fixed with 6, 4
    and 5 in `891b7d2`: `FleetLive` carries whether its overview is kept and
    `ShipLive` whether its crew detail is. The Ship and Starbase tabs say
    **Not current** for a kept overview, **Waiting** before the first, and
    **Not listed** only when a current overview lacks the ship; the crew
    says its detail is kept from an earlier read. Tests:
    `a_live_fleet_with_no_current_overview_never_says_not_listed`,
    `a_kept_crew_detail_says_so_rather_than_not_arrived`.
12. **A Retry within five seconds of the last read did nothing and said
    nothing** (`clients/fleet/src/directory.rs`). Real. Fixed with 1 in
    `c763063`: a
    Retry asked sooner is read once that interval ends, once however often
    it is asked, and never again on a timer. Test:
    `a_retry_asked_too_soon_reads_once_the_interval_ends`.
13. **The bridge test's "needs secrets" check asserted only an error**
    (`spaces/web/src/bridge/tests.rs`). Fixed in `f11117a` (`test(web):
    assert that the fleets need the secrets permission by its refusal`): it asserts the
    `DENIED` refusal for both the read and the retry.

### In play here

A captured row read just `NAV-6`, with no title or state. A reference's
row in **In play here** asked for nothing itself: only a transcript row
asks for its references when drawn, so a tile whose message was out of
view had no answer, and an answer other than a summary (pending,
unavailable, signed out, failed) also left the title and state empty. The
section now asks for its own references while the Ship tab shows, and a
row shows the key with its state when known, else **Loading** until the
answer comes and **Unavailable** when it cannot be shown (`93662d0`).
Tests:
`a_reference_in_play_says_its_state_once_answered`,
`an_unanswered_reference_says_loading_and_a_refused_one_unavailable`.

## Review findings

This pull request's own reviews: GPT-6.2 Sol (high effort, verdict not
clear: three high, six medium, two low) and Claude Opus 5.5 (high effort,
clear: three P2, five P3), both of head `83e124a`. Each was checked
against the code. Findings both raised are listed once.

### Fleet token store

1. **Keychain writes not ordered; a read started during a clear or save
   not dropped** (Sol high, Opus P2 1). Real. Fixed in `ff14d0f`: every
   read, save and clear runs on one ordered path (`service/queue.rs`);
   each waits for the one before and applies its result before the next
   starts, so the keychain and the service change in the order asked. A
   clear stops the token at once; nothing queued before it can restore it
   in memory or in the keychain. Tests (`service/tests/ordering.rs`, each
   over 40 schedules where order matters): clear then retry, retry then
   clear, save then clear, clear then save, the token stopping at once.
   Five of them failed on the code before; retry then clear already passed
   there and stays as a guard.
2. **A failed save during the first read left the fleet Checking** (Sol
   medium). Real. Fixed in `ff14d0f`: a token or address that cannot be
   saved is refused before the generation moves, and a save or clear that
   fails while current reads the keychain again. Test
   `a_failed_save_while_the_first_read_runs_settles_from_the_keychain`
   (failed before).
3. **The deferred Retry served only the first deadline** (Sol medium).
   Real. Fixed in `ff14d0f`: one waiting read per fleet the click covered,
   each at its own deadline, none for a fleet saved, cleared or removed
   since. Test `a_retry_reads_each_fleet_it_covered_once_its_own_interval_ends`
   (failed before).

### GitHub access

4 and 5. **GitHub access refusals, read ordering, opened pull requests and
   rate limits** (Sol high 2 and 3, Opus P2 2 and P3 8). Moved, with their
   fixes and tests, to `fix/github-reference-access`; not resolved here.

### Threads

6. **A follow-up request made as a reply never showed its replies** (Opus
   P2 3, Sol medium). Real. Fixed in `46bdbc6`: the tab shows the chosen
   request's own steps with the replies of the conversation's thread it is
   in, so the answers to it and a reply sent from its tab appear there,
   and the thread's count and the transcript's agree. Test
   `a_follow_up_request_shows_its_own_steps_and_the_replies_to_it`
   (failed before).
7. **Evicting a thread's root silently turned a reply draft into a channel
   message or command** (Sol medium). Real. Fixed in `46bdbc6`: the draft
   stays in reply mode, marked gone; the row says the thread is no longer
   loaded, Enter sends nothing, and only **Write to the channel** leaves
   reply mode, keeping the text. Tests
   `a_reply_whose_thread_is_gone_is_never_sent` and
   `a_reply_whose_thread_goes_is_kept_and_sent_nowhere_until_chosen`.
8. **A lone ending notice hid Load around this message** (Sol low). Real.
   Fixed in `46bdbc6`. Test
   `a_lone_ending_notice_still_says_the_earlier_steps_are_not_loaded`
   (failed before).

### The rest

9. **In play here not asked for when the Ship tab is chosen** (Sol
   medium). Real. Fixed in `2565a0e`. Test
   `choosing_the_ship_tab_asks_for_what_in_play_lists`.
10. **The Thread tab drew every reply each render** (Sol medium, Opus P3
    7). Real. Fixed in `2565a0e`: the latest 40 are drawn, the rest counted
    in a line; the heading counts all. Test
    `the_thread_tab_draws_the_latest_replies_and_counts_the_rest`.
11. **Linked status lines no longer announced as status** (Opus P3 4).
    Real. Fixed in `2565a0e`: a status element inside the button. Test
    `a_linked_status_line_is_still_announced_as_status`.
12. **The navigator kept absorption state for groups that disappeared**
    (Sol low). Real. Fixed in `2565a0e`. Test
    `a_group_that_goes_begins_to_take_in_its_rows_again_when_it_returns`.
13. **`want_in_play` between `inspector`'s doc comment and the function**
    (Opus P3 5). Fixed in `2565a0e`.
14. **A test claimed to test closing a view** (Opus P3 6). Corrected in
    `2565a0e`: the test and this record say it covers a view leaving the
    front; the release on close is not driven by a test.

### Second round

Second reviews of head `32fa227`: Claude Opus 5.5 (high effort, clear,
six P3) and GPT-6.2 Sol (high effort, not clear: three high, four medium,
two low).

**GitHub access** (Sol's three highs, the model test): moved, with
their fixes and tests, to `fix/github-reference-access`, and taken out of this branch, which
leaves the GitHub Space and the Framework's GitHub client as `main` has
them. Findings 15 to 18 are not resolved here.

**Fleet** (Sol medium, Opus P3):

19. **A failed clear brought the token back** (Sol medium, Opus P3). Real.
    Fixed in `43781b5`: the fleet says **Token not removed**, the token is
    not used, followed or shared, a Retry does not bring it back, Settings
    says the keychain may still hold it at the next start and offers
    **Clear token** again, and saving a token resolves it. Test
    `a_failed_clear_keeps_the_token_out_of_use_until_cleared_again_or_replaced`.
20. **A waiting Retry could read again right after a manual one** (Sol
    medium). Real. Fixed in `43781b5`: one re-read queued per fleet at most,
    and the waiting Retry checks the deadline again. Test
    `a_waiting_retry_never_reads_again_right_after_a_manual_one`.
21. **One queue for every fleet** (Opus P3). Real. Fixed in `43781b5`: one
    queue per fleet. Not tested: the test executor cannot hold one keychain
    call while another runs.

**Channels** (Sol, Opus P3):

22. **Escape dropped a gone reply's guard** (Sol medium). Real. Fixed in
    `2e2b108`; the render test sends `/part`, presses Escape, sends `/part`
    again, and nothing is sent. Deleting the text does not lift the guard:
    only **Write to the channel** does.
23. **A held terminal notice overrode the kept ending** (Sol medium). Real.
    Fixed in `2e2b108`. Test `a_kept_ending_is_the_end_whatever_notice_is_held`.
24. **A follow-up request's tab count disagreed with its status line**
    (Opus P3). Real. Fixed in `2e2b108` by wording: its heading reads
    **Replies in this thread**; its own status line counts none, as only a
    thread's first message carries the count.
25. **`MAX_REPLIES` under the replies doc comment** (Opus P3). Fixed in
    `2e2b108`.
26. **The docs said only Write to the channel leaves a gone reply, while
    Escape did** (Opus P3). Now true: fixed by 22.
27. **A status element nested in a button may not be announced** (Opus P3).
    Fixed in `35083bc`: the status element sits beside the button, which
    still draws and opens the whole line.
28. **The fixture's JOIN race** (Sol low). Real. Fixed in `5da66b0`. Not run
    against a server in this round.
29. **The trust paragraph addressed reviewers** (Sol low). Reworded in this
    commit to state the owner's rule and its reason only.

## Captures

Captured on macOS with `make capture` at 1440 x 900 in Twilight Two, with
`CAPTURE_FLEET=1 CAPTURE_FLEET_API=1`, against a fresh loopback Ergo 2.19.1
(`scripts/irc-test-server.sh` with `IRC_TEST_LISTEN=127.0.0.1:16951`) and
`scripts/irc-history-fixture.py --status --thread`, both stopped by their
process ids afterwards, and a Channels config naming that server as the
network **Acme IRC** with the aurora channels. Every name, issue and message
is fictional. The fixture plays the requests a few seconds apart, so the
durations are seconds, not the mock-up's minutes. Steps:
`wait:9000;click:70,215;wait:4000;shot:status-replies;click:1194,55;wait:1500;shot:thread-prompt;click:340,375;wait:2000;shot:thread-finished;click:380,591;wait:2000;shot:thread-running;click:340,375;wait:2000;click:1196,851;wait:1000;type:Thanks, that settles the baseline.;wait:800;shot:reply-mode`.

The first image was taken again after the review fixes, with the same
fixture on `IRC_TEST_LISTEN=127.0.0.1:16961`, a config that also joins
`#aurora-command` and every ship's channel so the navigator lists all four
of the roster's ships, and the steps
`resize:1440,900;wait:9000;click:70,247;wait:5000;shot:status-replies`.

![The ship's channel: mira's finished request reads "Done in 11 s · 3 replies ›" and her running one "aurora-lyra · Using a tool … working"; Ships lists castor, lyra, orion and vega; the inspector's tabs read Ship, Thread, People 6, and the Ship tab's card, Crew, In play here (each row with its title and state) and Here now start at the inspector body's text edge, as the other tabs do](evidence/status-replies.png)

![The Thread tab before a thread is chosen: one line saying to choose a message's replies or a request's status line](evidence/thread-prompt.png)

![The Thread tab on the finished request: the root, Status with Accepted, Working, Writing the reply and Done in 11 s, then Replies · 3 grouped by author, and Reply in thread in the footer](evidence/thread-finished.png)

![The Thread tab on the running request: Accepted, then Working since 22:29 with no duration yet, and its one reply](evidence/thread-running.png)

![Reply mode: above the input, "Replying in thread to mira:" and the start of the request, with × to cancel, and the draft typed](evidence/reply-mode.png)

Each image was compared with the design guide's shell anatomy: the tabs sit
in the inspector header on the strip line, the open tab shows by its surface
and stronger text, the footer's action is in the inspector footer, text
starts at the panel's edge, and no image has an accent line on one edge.

## Not verified

- A light theme, a narrow window and a Starbase's Thread tab: covered by
  tests, not captured.
- **Load around this message** against a real gap: the capture fixture has
  no stored ending without notices; tests cover the note, not the load.
- The refusal footer without `message-tags`: covered by a render test, not
  captured, since the loopback Ergo agrees it.
- A sent reply reaching other clients: `Command::Reply` and the client's
  tagging are tested; no second client was watched in a capture.
- A real Stargate: only the fixture's tags.

## Tasks

- [x] Thread tab with root, timeline and replies.
- [x] Reply counts and status lines that open it; Enter on a message.
- [x] Chosen thread per conversation, rebuilt only on change.
- [x] Reply mode to the channel with the reply tag, refusals.
- [ ] Who closed what, delegation and pull request steps, once the progress
      contract carries them.
- [ ] Server-side reply counts (the reply-search follow-up).
- [ ] Attachments in a thread reply.
