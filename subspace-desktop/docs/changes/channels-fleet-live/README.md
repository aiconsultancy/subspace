# Channels fleet live

## Intent

The owner's mock-up of the Channels redesign shows a ship's channel with
its fleet's live state: the header pill `Lyra is delegating · 3 of 4 crew
busy`, an inspector with **Ship**, **Thread** and **People** tabs (the
ship's card, its crew and what each is on, what is in play in the
conversation, the last decision and who is here), and a ship row's one dot
that says whether its crew is working (frame 2: the tooltip names who and
on what). This change builds them from the app's native Fleet API client
(`clients/fleet`, [Fleets and the Fleet API](../../fleet-api.md)), and fixes
the findings carried from pull requests 66 and 68.

Desktop only: no Framework, API or Stargate change.

## Findings carried from pull request 66

Four low findings from the reviews of the Fleet API client.

1. **Fleet Console's Retry for a token the keychain could not read** —
   fixed (`fix(fleet-client): read an unreadable token again when Fleet
   Console retries`). The banner says to unlock the keychain and choose
   Retry; the service read each token once, so only a restart or a fresh
   save helped. The bridge's `fleets.get` now calls
   `FleetApi::reload_unreadable`, and the page hears
   `subspace:fleets-changed` when the read lands. Test: a locked stand-in
   keychain, unlocked, read again on retry.
2. **`--log-requests` raising on a malformed request** — fixed
   (`fix(capture): log a malformed request to the Fleet API fixture without
   failing`): `getattr`, `-` and `auth=none`. Test: a raw malformed request.
3. **Control characters in the logged method and path** — fixed
   (`fix(capture): escape control characters in the Fleet API fixture's
   request log`). Test: an escape sequence in both.
4. **`docs/capture.md` on when the token is saved** — fixed
   (`docs(capture): say the fleet's token is saved whenever
   CAPTURE_FLEET_API is set`), with the script's header.

## Findings carried from pull request 68

Thirteen medium and low findings from the two model reviews of
[Channels tiles once](../channels-tiles-once/README.md). Each was checked
against the code.

1. **A tile vanishing when its owner is no longer shown** — fixed
   (`fix(irc): let go of a tile whose owner is no longer shown`). Each read
   lets go of owners the conversation no longer shows (the line cap, a
   progress notice folded into its request); a removed conversation's
   owners are forgotten (`Sessions::forget` from `Hub::remove`); a
   conversation whose buffer the hub does not hold reads nothing, so its
   owners are not let go. Only unread messages claim, so no drawn message
   grows. Tests: an owner dropped then a new mention; remove and rejoin.
2. **The in-app help describing the old chips** — fixed (`docs(irc): describe
   tiles once in the in-app help`): the issue and pull request sections now
   describe the tiles, their loading and unresolved states.
3. **Work in proportion to every held message on each sync** — fixed with 1:
   a sync with nothing new (`Tiles::stale`) neither copies the owners nor
   the read set, and the conversation drops its own handle first. The sets
   are still built per sync, in proportion to the messages shown; a
   watermark was not needed for the 2,000-line cap.
4. **A muted Unavailable tile for every pull request while no Space
   answers** — left, as intended: the tile's box is reserved from the
   message's first frame so nothing moves, and removing it once the refusal
   lands would move every line under it. The help and the design notes now
   say so.
5. **A long owner name pushing the due date out** — fixed (`fix(irc): let an
   issue tile's owner give way, and meter only trackers`): the owner is the
   part that shrinks with an ellipsis, as an assignee is.
6. **A selected row moving into a group already folded** — fixed (`fix(irc):
   open a folded fleet group the selected row moves into`): the board
   remembers the group that held the selection; a folded group the selection
   has just moved into opens, one folded by hand around it stays folded.
7. **The name clash check being case-sensitive** — fixed (`fix(irc): tell
   navigator names apart ignoring case, in one pass`).
8. **Ownership growing for the session and cloned on each sync** — fixed
   with 1 and 3: owners no longer shown are let go, so claims are bounded by
   the messages shown, and nothing is copied without news.
9. **Issue ownership ignoring the tracker scope** — fixed with 1: a new
   recognition lets go of every issue's claim and the messages, all read
   again, claim within the new scope; pull requests keep their owners.
10. **A Starbase `/ask` with two rear admirals hidden by a nick clash** —
   fixed (`fix(irc): ask no rear admiral in a Starbase whose fleet names
   two`): the roster counts each Starbase's rear admirals before dropping
   clashes.
11. **The loading skeleton's bars exceeding a narrow column** — fixed
   (`fix(irc): keep a loading tile's skeleton bars inside a narrow column`):
   each bar is capped at the column's width. Not covered by a render test.
12. **A progress meter without the tracker flag** — fixed with 5.
13. **The quadratic name clash check** — fixed with 7: titles are counted in
   one pass.
