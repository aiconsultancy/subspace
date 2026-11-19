# Sign in with GitHub: intent

The owner asked for an in-app **Sign in with GitHub**, so people no longer
need the GitHub CLI (`gh`) or a personal token to use the GitHub Spaces, the
updater and Copilot chat.

Owner decisions:

- A GitHub **OAuth App** (not a GitHub App), client ID
  `Ov23lihtCHEoDujzEB6y`, registered callback `http://127.0.0.1:31415/callback`.
  For OAuth Apps with a loopback callback GitHub accepts any port, so the app
  listens on a free one.
- Scopes `repo read:org` (gh's defaults without `gist`).
- The client secret ships inside release builds, as GitHub Desktop does. It is
  never in the repository: release packaging reads it from the login Keychain
  (`subspace-github-oauth` / `client-secret`) and a stable release fails
  without it. Builds without it use the device flow.
- Keep `gh` as a fallback. Precedence becomes environment, config file,
  signed-in account, then `gh auth token` per account.

The design follows Zed's sign-in (loopback listener, `open_url`, activate the
window on return, a credentials provider with a development store and a test
fake), adapted to GitHub's OAuth App flow with PKCE.
