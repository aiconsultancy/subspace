# Sign in with GitHub: spec

## Flow (`framework/services/github-auth`, `subspace-github-auth`)

Headless, no GPUI, depends only on services.

- **Browser** (client secret compiled in): authorization code with PKCE S256
  (RFC 7636) and a random `state`. Opens
  `https://github.com/login/oauth/authorize?client_id&redirect_uri&scope&state&code_challenge&code_challenge_method=S256&prompt=select_account`.
  A one-shot `std::net` listener on `127.0.0.1:0` answers `/callback`: it
  checks `state`, answers the browser with a small self-contained page
  (signed in, or did not finish; it never echoes the request) and hands each
  connection to its own thread so a browser's idle speculative connection
  cannot hold up the real one. Five-minute limit and Cancel. The code is
  exchanged at `/login/oauth/access_token` with `client_id`, `client_secret`,
  `code`, `redirect_uri` and `code_verifier`.
- **Device** (no secret, RFC 8628): `/login/device/code`, then polling with
  `grant_type=urn:ietf:params:oauth:grant-type:device_code`, honouring
  `interval`, `slow_down` (+5 s or GitHub's new interval), `expired_token`,
  `access_denied` and `device_flow_disabled` (a plain message, not a crash).
- Both: the granted `scope` must include `repo` and `read:org` (`write:org`
  or `admin:org` count for `read:org`), else the sign-in fails naming the
  missing scopes. `GET /user` names the account.
- Sign-out revokes best effort with
  `DELETE /applications/{client_id}/grant` (basic auth) when the secret is
  compiled in.
- Debug builds only: `SUBSPACE_GITHUB_OAUTH_URL` and `SUBSPACE_GITHUB_API_URL`
  may name numeric loopback HTTP addresses (fixtures).
- Errors are plain words and never carry a token, code or secret. Requests go
  through `subspace_core::http::agent()`, whose log never has a query or body.

## Storage and precedence (`subspace-core`)

- `credentials::signed_in::SignedInAccounts`: logins in `github-accounts.json`
  beside the state file (not `state.json`, which the GPUI `prefs` global
  rewrites whole, so a second writer would lose updates); tokens per lowercase
  login in the system keychain under `<data directory>.github` (release), an
  owner-only `github-tokens.json` (debug, unless
  `SUBSPACE_DEVELOPMENT_USE_KEYCHAIN`), or memory (tests).
- `keychain::Keychain` is the shared system store; NETT's
  `KeychainCredentialStore` now delegates to it.
- `Credentials::github`: environment, config file, then `account(login)`:
  the signed-in token, else `gh auth token --user`. `github_accounts()` merges
  signed-in and gh logins once each, signed-in first in GitHub's spelling, and
  works without gh. `AccountSource` says Signed in, Sign-in expired or
  GitHub CLI.
- A 401 on a signed-in token (`invalidate`) marks the account expired unless
  it has signed in again since with another token. An expired account falls
  back to gh's account of the same login, else asks to sign in again.
- Consumers: the GitHub client (PRs, Actions, Copilot Agents, account and EMU
  selection; session logs accept a signed-in OAuth token), the updater
  (app variable, `GH_TOKEN`, `GITHUB_TOKEN`, signed-in, then `gh`), and
  Copilot chat's identity (`account(login)`).

## UI

- `subspace_dashboard::github_sign_in`: `GitHubSignIn`, the app's one sign-in
  (installed by the shell), with `start`, `cancel`, `sign_out` and
  `SignInEvent`; the token goes straight to the credentials' store.
  `status` and `sign_in_button` are the shared component (gallery card
  `github_sign_in::status`). The browser opens with `open_url`; after a
  browser sign-in the window comes to the front (`activate(true)`). A device
  code never opens the browser by itself.
- Settings, every page with `ConnectionField::GitHubCredentials`: **GitHub
  accounts** lists each account with its source, **Sign out** for signed-in
  ones, **Sign in with GitHub** and the running sign-in's status. Sign-in and
  sign-out apply at once; the copy says so. The **Account** select lists
  every account and still waits for Save.
- Chat: `Event::SignInRequired(SignInOffer)`. No account, an expired or
  rejected sign-in, or a rejected non-active gh account offer the in-app
  sign-in (the shared status shows under the error, and the panel reconnects
  after it); gh's rejected active account keeps `gh auth refresh`.

## Release

`scripts/github-oauth-secret.sh` reads the secret from
`SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET`, else `security find-generic-password -s
subspace-github-oauth -a client-secret -w`, never printing it.
`release-macos.sh` reads it first, removes it from its environment, refuses a
stable release without it and passes it only to the packaging.
`package-macos.sh` passes it only to the Cargo build, warns without it, and
fails if the built executable does not contain it (a stale build cache).
`build.rs` declares `cargo:rerun-if-env-changed`.
