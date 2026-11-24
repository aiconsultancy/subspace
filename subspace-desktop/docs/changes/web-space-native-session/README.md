# Web Spaces signed in with the app's account

## Intent

A product built on Subspace may ship a web Space for its own site, and the
person has already signed in to the app under **Settings › Account**. Asking
them to sign in again on the page is friction, and the page's own sign-in
keeps a long-lived session on disk. The owner wants a generic way for a
product to compose such a Space: the product's trusted native adapter turns
the account into a short-lived session cookie the site issues, and the web
Space shows the site with only that cookie. It must stay off unless a
product composes it and never let a token reach a page script, an address,
a log or the view's storage.

An earlier draft blocked the UI thread while installing the cookie (Wry's
synchronous cookie calls spin the run loop inside a GPUI update), revoked
sessions on a detached thread that died with the process, and kept its
lifecycle in a bag of flags. This change fixes all three.

## Spec

- **API, macOS only.** `subspace_web_board::create_with_session(board,
  NativeSession::new(scope, account, acquire), window, cx)`. `SessionScope`
  (home, signal, reserved routes) says what the view may load;
  `SessionAccount` is the product's account, which the product implements
  on a newtype around the shell's `AccountSession`, because a Space cannot
  name the shell; `acquire` turns it into a `SessionGrant` (a
  `SessionCookie`, a lifetime of at most `MAX_SESSION_LIFETIME`, and an
  optional revocation) or a `SessionFailure`. The API, the session's view
  and its cookie store are compiled for macOS only, decided by one
  `cfg(target_os = "macos")` on the module and the page's session parts.
- **Lifecycle.** `browser_session/machine.rs` is a state machine with one
  `Phase` (signed out, ready, acquiring, installing, live, ended). Each
  transition takes what happened (account, clock, the adapter's answer, the
  store's answer, the signal, Retry), the time and where the view is (a
  page in the scope, or none), and returns effects (acquire, open a view,
  install, show the view opened for an attempt at an address, drop a view
  that never showed, report renewal, return to an address, close, notice,
  log).
  `browser_session/board.rs` applies them. A session that ends keeps where
  the view was, so Retry reopens there; another account starts at the
  home page. The tests drive the transitions.
- **Where the page comes from.** `pages.rs` holds `PageSource` (the config
  file, or a session). The board is built from it, which says where the
  view starts and whether it has a bridge, and asks it to open, retry and
  reload the page, for Open in browser's target and for what to show
  without a page. The board keeps its one 100 ms poll.
- **View.** One description (`page/native/view.rs`: builder, first address,
  navigation rule, app files, developer tools) builds every view. A
  session's has a new in-memory WebKit store, no app files, bridge,
  initialisation script or developer tools, starts at `about:blank`, and
  loads the scope only once the store confirms the cookie. The board keeps
  a new view with its attempt until the session shows it, closes, or opens
  a newer one, and drops it at once, with its store, when the store
  refuses its cookie or does not confirm it in time. A view that opens for
  an attempt that no longer applies is dropped unused, and a session that
  goes live without its view ends, with Retry, rather than wait behind
  "Opening the page…". Page events left in a batch after the page closed,
  such as the title of a page whose session the signal ended, are not
  applied.
- **Cookie.** The Space validates the name and value and sets every
  attribute: host-only on the scope's host, the scope's path or a narrower
  one inside it, `Secure`, `HttpOnly`, `SameSite=Strict`, expiring with the
  grant. A path or name the product got wrong is logged, without the value,
  and not blamed on the site. `Debug` hides the value.
- **Asynchronous install.** `page/native/cookies.rs` calls
  `WKHTTPCookieStore` `setCookie:completionHandler:` and `getAllCookies:`
  from the UI thread and returns a plain future at once; WebKit's completion
  block answers a one-shot channel. The read-back checks every attribute,
  that the cookie is not session-only, and its expiry to within a second.
  The board bounds the future with a 10-second timeout, and the adapter's
  attempt with a 60-second one, through GPUI's `FutureExt::with_timeout`.
  No call waits on or spins the run loop.
- **Navigation.** The view starts on `about:blank`, refused once a page in
  the scope loads so Back stops there, and loads HTTPS pages on the home's
  origin under its path. It refuses other origins and schemes, credentials,
  empty and dot segments, backslashes, `;`, percent-escapes of unreserved
  characters, separators, `%`, control characters and anything outside
  ASCII, reserved routes (ignoring ASCII case, and with or without a format
  suffix such as `.json`) and frame documents such as
  `about:srcdoc`, `blob:` and `data:`, for every navigation Wry reports in
  any frame, redirects included. Reserved routes limit navigations only; a
  page's `fetch` still carries the cookie. A cookie path must keep the
  prefix's final `/`, so the bare prefix never receives it.
- **Signal.** The view never loads the signal. The session renews at once
  and the page loads again; a second signal within 30 seconds, or one after
  renewing failed for good, ends the session, and one while a failed
  renewal waits to retry waits with it. A signal that waited, or whose own
  renewal failed and is retried, loads the page again and starts the
  30-second guard once a renewal succeeds. There is no separate probe.
- **Redraws.** A transition redraws the Space only when it has effects or
  changes what the Space shows, so an idle session costs no redraws.
- **Renewal.** Two minutes before the end, or at half life; grants shorter
  than `MIN_SESSION_LIFETIME` (4 minutes) are refused, so renewals never
  loop; three retries of a temporary failure; a store that refuses a
  renewal, or does not confirm it within 10 seconds, ends the session and
  closes the view, since a late write could otherwise replace the live
  cookie with a revoked one. Each renewal's grant is a new session, and the
  old one is revoked once the new one is confirmed.
- **Revocation.** A grant's revocation runs once, on its own thread, when
  the grant is dropped: renewal, end, expiry, account change, sign-out, a
  stale answer, or the Space closing. An app quit hook, registered once by
  `create_with_session`, revokes every live grant and waits at most two
  seconds. A second `with_revoke` replaces the first, which never runs.
- **Platforms.** WebView2 has an asynchronous cookie manager, but the
  capability is not built or tested there; Windows and Linux builds have
  none of the API.
- **Every web Space.** Two changes reach web Spaces without a session: the
  status bar says `Blocked a page this Space does not show.` for a refused
  HTTPS address (one with credentials), where it said `Blocked a https:
  link`; and Open in browser is enabled only when the page's address is a
  web page.

## Delivery

- Tests: `browser_session/tests.rs` and
  `tests/{renewal,signal,grant,board}.rs`
  drive sign-in, the first attempt waiting until shown, a new view's
  install confirmed, refused and timed out, renewal timing, a renewal
  confirmed, refused, timed out and failing, expiry by either clock and
  during a renewal, the signal and its guard, account switch and sign-out,
  stale answers, product and site cookie mistakes, the maximum lifetime,
  exact `Debug` output, which attempt may keep a new view, where the view
  returns after the signal and resumes after expiry, a second signal or a
  refused renewal (and not after another account signs in), and in a test
  window a configured Space without a session, sign-in required with Open
  in browser on the board's own page, a failure, Reload as Retry, a grant
  revoked unused, resuming only at a page in the scope, a session whose
  view is gone when it goes live, and the quit hook revoking. No test opens
  a real web view. `browser_session/{scope,revocation}.rs` and
  `page/native/session.rs` test the scope (including `//session`,
  `;x`, `%73ession`, case, `%C5%BF` and `%E2%84%AA`), the blank page refused
  after the first page, revocation (including a replaced revocation that
  owns a grant) and the view's admission. Further tests cover the minimum
  lifetime, a renewal the store does not confirm in time, signals after a
  failed renewal, an idle session that never redraws, and quitting that
  returns only after a slow revocation.
- Review fixes, after the first pull request merged: its Claude Opus 5.5
  review found that a signal waiting for a failed renewal's retry was
  dropped, so the page never loaded again; that a new view the store
  failed stayed hidden, holding a revoked grant's store, until Retry; that
  events left in a batch could bring back a closed page's title or notice;
  that reserved routes missed format suffixes; and that this reference
  claimed no trace in builds that still ship the in-app guide. Each is
  fixed, with a test for each code fix. A same account that stops being
  current while a sign-in is in progress, then comes back, starts again
  at the home page, by design.
- `cargo test`, `cargo clippy --all-targets -- -D warnings` and the format
  check for `subspace-web-board`; `cargo test` for `subspace` and
  `subspace-descriptor`; the architecture, board-guide, documentation-index
  and quality checks.
- WebKit, outside the app: a scratch probe built the cookie exactly as
  `cookies.rs` does and set it in a non-persistent `WKHTTPCookieStore`. The
  store returned the domain (no leading dot), path, `Secure`, `HttpOnly`,
  `SameSite=strict` and a non-session expiry unchanged, the expiry cut to
  whole seconds, so the read-back accepted it; a renewal with the same name
  and path replaced it in place. A second probe loaded a loopback fixture
  page in a `WKWebView` over that store: the request to `/desktop/` and
  `/desktop/reports` carried the cookie, `/desktop` and `/other/` did not,
  and `document.cookie` was empty.
- Not checked here: the whole app against an HTTPS site (the Space's
  navigation handler with redirects, the signal, renewal, account switch
  and quit), and Windows and Linux builds. No product in this repository
  composes the capability, so there is no screenshot of the sign-in state;
  a product that composes it captures its own.

## Tasks

- [x] `SessionScope`, `SessionAccount`, `NativeSession`, `SessionCookie`,
  `SessionGrant`, `SessionFailure` and `create_with_session`.
- [x] The session as a state machine with effects; `PageSource`.
- [x] One view description; isolated, in-memory session view.
- [x] Asynchronous cookie install and read-back, raced with a timer.
- [x] Renewal, expiry, the signal, account change and sign-out.
- [x] Revocation on drop and at quit, bounded.
- [x] Tests, help, reference and builder guide.
