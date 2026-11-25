# Web Spaces signed in by the app that follow the app

## Intent

A product that composes a web Space signed in with the app's account
(`create_with_session`) often has a choice elsewhere in the app that the
site's pages mirror, such as the team or project the person is working on.
Today the Space always starts at the scope's home, so the person must make
the same choice again on the page, and again after each restart, because
the session's view keeps nothing on disk. The owner wants a generic way for
a product to say where the Space's view starts, derived from the app's own
state, and to have the view follow when that state changes.

## Spec

- **API, macOS only.** `NativeSession::with_start(route)`, where `route` is
  `Fn(&App) -> Option<String>`: a reference relative to the scope's home,
  such as `?team=blue` or `teams/blue`, or `None` for the home itself.
  Without it, a session starts at the home, as before.
- **Reading the route.** The board reads it at each poll (100 ms), on the UI
  thread, and passes it to the state machine only when its answer changes,
  after the account and the clock, so a session that ends in the same poll
  still forgets the page from before the move.
- **Resolving it.** `SessionScope::resolve` joins the route to the home as a
  browser would and keeps it only if the view may load it: a page in the
  scope, not `about:blank`, the signal or a reserved route. Otherwise the
  start is the home and the machine returns `Effect::Log` once, naming no
  route.
- **Moving the start** (`Session::route`). When the resolved start differs
  from the current one:
  - a view showing a page, live or renewing, loads the new start at once
    (`Effect::Load`, which also carries the page the signal returns to and
    replaces `Effect::Return`);
  - while the site refuses the session (the signal's renewal is under way,
    or the signal waits for a failed renewal's retry), a load now would be
    sent back to the signal, so the view loads the start once a renewal
    succeeds;
  - the session remembers the page the view showed when the start moved
    (`Session::left`) until it next goes live or ends. A load the site
    sends to its signal leaves the view on that page, so neither the
    signal's return nor **Retry** goes back to it: the start opens
    instead. A view that reached another page after the move returns and
    resumes there as before; one that went back to the very page the move
    left, by Back or a link, counts as not having moved, until the
    session next renews or ends. Telling those apart would need committed
    navigations, not the polled address, which WebKit may report for a
    load still in progress;
  - a session with no view yet, a new view under way, and a session that
    ended stop resuming where the old view was, so the new view, or
    **Retry**, opens the start;
  - a new view that has no page to resume, and a signal's return when the
    view has no page in the scope, use the start instead of the home.
- **Go to the start page** loads the current start.
- Nothing else changes: account changes, renewal, the signal, expiry and
  revocation behave as before, and a session without `with_start` is
  unchanged.

## Delivery

- `browser_session/tests/start.rs` and `tests/moved.rs` drive the machine:
  a new view starting at the route, a live view and a renewing one moving
  at once, an unchanged route doing nothing, the home again, Retry and a
  new view under way opening the moved start rather than where the view
  was, the signal's fallback, a move while the site refuses the session
  (during the signal's renewal, before and after its install, and while
  the signal waits for a failed renewal), a move whose own load the site
  sends to its signal (renewing, or ending within the signal's 30
  seconds), a move after renewing failed for good, a view that reached
  another page after a move, another account or expiry just before a move
  in the same poll, a later session returning to its own page, and routes
  outside the scope logged and starting at the home. `scope.rs` tests what
  resolves (queries, paths, fragments, absolute addresses in the scope)
  and what does not (other origins, `..`, `//`, `about:blank`,
  `javascript:`, the signal and reserved routes, with format suffixes and
  in another case). In a test window, `tests/start.rs` also checks that Go
  to the start page follows the route and that moving the start starts no
  attempt.
- Deliberately breaking each part (no restart, no load, a new view at the
  home, the signal back to the home, no log, `about:blank` allowed, the
  board not following the start, an ended or fresh session keeping where it
  was, loading while the site refuses the session, either refusal not
  counting, not remembering or not dropping the page a move left, the
  signal's return or Retry going back to it) fails at least one of these
  tests. Passing no page from the board to the route is not caught: no
  test opens a real web view, so the board's tests have no page.
- Review: read-only Claude Opus 5.5 (high) reviews found no P0 or P1. The
  first round's P2s (a move during the signal's renewal undone when it
  completed; review material missing two new files) and P3s (the poll's
  order, the help's Retry wording, route path rules, this record's test
  file, missing tests) are fixed here. The second round's P2 (a move whose
  own load the site sends to its signal undone the same way) and P3s (a
  flag that outlived the move it guarded, a long help line, the reference
  saying "dot segments" where the scope refuses any `..`) are fixed by
  remembering the page the move left instead of a flag. The third round
  found no P0, P1 or P2; its P3s (a return to the very page a move left,
  now documented here and in the help; no test of an ordinary renewal
  forgetting that page, now added) are addressed.
- `cargo test`, `cargo clippy --all-targets -- -D warnings` and the format
  check for `subspace-web-board`; the architecture, board-guide,
  documentation-index and quality checks.
- No product in this repository composes a native session, so nothing the
  app shows changes and there is no screenshot. A product that uses
  `with_start` captures its own. Windows and Linux have no native session.

## Tasks

- [x] `NativeSession::with_start` and `SessionScope::resolve`.
- [x] `Session::route`, the moved start in `go_live`, and `Effect::Load`.
- [x] The board reading the route each poll, and Go to the start page.
- [x] Tests, reference and in-app help.
