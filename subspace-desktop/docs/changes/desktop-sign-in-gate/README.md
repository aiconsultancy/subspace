# Sign-in at start-up

## Intent

A fresh install of a build with an API backend opened on Spaces that each
said sign-in was required, and the person had to find Settings › Account.
The owner wants the app to ask first: when the API backend is on and there
is no session, a sign-in dialog over the main window that has to be
answered before the app is usable. Quitting stays possible. A build with no
API backend never shows it.

## Spec

- **Who asks.** The Framework's account gains
  `AccountRegistration::required`. The product sets it for every account it
  registers, which only a build with an API backend does (`src/identity.rs`).
  A build with no API backend registers no account, so nothing asks.
- **When.** The shell asks while the required account has no session and
  its provider needs a person to sign in (`ProviderInfo::browser`). The
  session is kept apart from the outcome of the last sign-in attempt
  (`account::Session`, `account/rules.rs`): only restoring or renewing a
  saved session, signing in and signing out change it.
  - It asks once start-up finds no saved session, after a sign-out, and
    when a session can no longer be renewed (`SignInRequired`,
    `InvalidReply`).
  - A sign-in attempt that fails, times out, is retried or is cancelled
    never ends the asking: only a session lets anyone in.
  - It never asks while the saved session is being restored. A saved
    session that cannot be checked because the network, the identity
    provider or the keychain is out of reach (`Offline`, `Service`,
    `Keychain`) opens the app, where Settings › Account and the Spaces show
    the existing retrying state. A 5xx, 408 or 429 from the provider is
    such an outage, whatever its body (`http::status::is_transient`, used
    by the Entra transport and the WorkOS failure mapping); a refused grant
    still ends the session.
  - The `dev` provider signs in by itself and never asks.
- **What.** `subspace-framework/host/app/src/app/sign_in_gate.rs` (and
  `sign_in_gate/view.rs`) draws it with the shared dialog treatment
  (`dashboard::dialog::backdrop` and `frame`, `action::dialog_button`): the
  title **Sign in to Subspace**, a line naming the organisation from the
  build's branding, the account's progress text from Settings › Account,
  and Quit, then the provider's sign-in, or while it runs, Cancel and Open
  browser again. Retry appears after a sign-in that could not reach the
  service. The backdrop occludes the window, and the board is told it is
  covered. The dialog has the dialog role and is labelled with its title.
  Nothing dismisses it; Quit, or Cmd/Ctrl+Q, quits as the app always does.
- **Nothing behind it acts.** While it shows, the shell attaches only Quit
  and Quit and Restart: every other shell, Space-switch, terminal and chat
  action, from a key or the menu bar, finds no handler. The entry points
  the toolbar's buttons call directly (Space switching, the palette, chat,
  the terminal, Settings, About) do nothing either. When it appears it
  closes the palette, the shortcuts, Settings and any dialog.
- **Focus.** Each control has its own focus handle and the frame one of its
  own. Tab and Shift-Tab cycle among the controls it shows in every state
  (signed out; starting; waiting with the device code; failed with Retry).
  Whenever focus is not on one of them, as when the focused control goes
  because sign-in started, when it leaves the frame, or when the window is
  activated from the background, it moves to the main control. While the
  quit confirmation is open over it, the gate leaves focus to the
  confirmation, whose two buttons Tab moves between. When it closes,
  focus returns to the Space on show.
- **Routes from outside the window.** A notification clicked while sign-in
  is due, or may be once the saved session is restored, is held and opened
  once signed in, instead of opening a conversation behind the dialog.
- **After.** Signing in closes it. Channels' account networks
  (`spaces/irc/src/managed`) follow the account as they already did, so
  they connect with no settings touched.
- **Capture.** `account:required-<state>` seeds a state for an app that
  requires its account, so the dialog can be captured
  ([Desktop capture](../../capture.md)).

Not done here: loading who the person is, their fleets and initiatives and
the account networks in one pass behind a loading state before the window
shows; after sign-in the app carries on and each part loads as before.

## Delivery

- Tests, all under `subspace-framework/` unless named:
  - `host/app/src/account/tests.rs`: what changes the session, and that no
    session, a failed sign-in, Retry and Cancel keep asking throughout;
    a required account asks again after sign-out.
  - `host/app/src/app/shell/cover_tests.rs`: the dialog covers the window
    only while there is no session; Settings, the palette, the terminal,
    chat and the Space switches are refused while Quit acts; Settings
    closes when it appears; focus returns to the Space after sign-in.
  - `host/app/src/app/shell/sign_in_focus_tests.rs`: Tab and Shift-Tab
    cycle the gate's own controls and reach Quit in each state; focus stays
    in the gate when its focused control goes; the quit confirmation over
    it answers the keyboard and completes; the toolbar's entry points are
    inert; a notification waits for sign-in, then opens once.
  - `services/core/src/http.rs`, `services/entra-auth/src/http.rs` and
    `services/workos-auth/src/http.rs`: a 5xx, 408 or 429, with any body,
    is an outage and not a refused grant.
  - `host/app/src/app/sign_in_gate/view.rs` (the organisation line) and
    `subspace-desktop/products/subspace/src/identity.rs` (both API backends
    require their account).
- Screenshots, in Twilight Two at 1440 x 900, from the capture build with
  `account:required-<state>` over the PRs Space:

| Image | Shows |
| --- | --- |
| [No session](evidence/sign-in-gate.png) | Start-up with no session: the organisation, Quit and the provider's sign-in |
| [Device sign-in](evidence/sign-in-gate-device.png) | Waiting for the browser, with the code to confirm, Cancel and Open browser again |
| [Expired](evidence/sign-in-gate-expired.png) | A session that can no longer be renewed: Sign in again |

## Tasks

- [x] `AccountRegistration::required` and when a session is needed.
- [x] The dialog over the main window, from the shared dialog treatment.
- [x] Required in both API-backed builds; none in a build with no backend.
- [x] Tests, capture state, screenshots and docs.
- [ ] A single loading pass after sign-in.
