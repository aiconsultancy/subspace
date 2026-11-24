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
- **When.** The shell asks while the required account has no usable session
  and its provider needs a person to sign in (`ProviderInfo::browser`;
  `account::needs_session` in the Framework):
  - after start-up restored no saved session;
  - after a sign-out, or when a session can no longer be renewed
    (`SignInRequired`, `InvalidReply`);
  - throughout the sign-in it started, including after that sign-in fails.
  It never asks while the saved session is being restored, and a saved
  session that cannot be renewed because the network, the service or the
  keychain is out of reach (`Offline`, `Service`, `Keychain`) opens the app,
  where Settings › Account and the Spaces show the existing retrying state.
  The `dev` provider signs in by itself and never asks.
- **What.** `subspace-framework/host/app/src/app/sign_in_gate.rs` draws it
  with the shared dialog treatment (`dashboard::dialog::backdrop` and
  `frame`, `action::dialog_button`): the title **Sign in to Subspace**, a
  line naming the organisation from the build's branding, the account's
  progress text from Settings › Account, and Quit, then the provider's
  sign-in, or while it runs, Cancel and Open browser again. Retry appears
  after a sign-in that could not reach the service. The backdrop occludes
  the window and sets the modal key context, so the Space behind takes no
  pointer or shortcut, and the board is told it is covered. Focus moves to
  the sign-in button when the dialog appears. The dialog has the dialog
  role and is labelled with its title. Nothing dismisses it; Quit, or
  Cmd/Ctrl+Q, quits as the app always does.
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

- Tests: `subspace-framework/host/app/src/account/tests.rs` (when a session
  is needed, and a required account with a browser sign-in asking again
  after sign-out), `app/shell/cover_tests.rs` (the dialog covers the window
  and takes focus only while a required account has no session, and not
  when signed in or offline with a saved session), `app/sign_in_gate.rs`
  (the organisation line) and `products/subspace/src/identity.rs` (both
  API backends require their account).
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
