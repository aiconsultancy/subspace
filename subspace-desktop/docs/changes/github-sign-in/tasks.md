# Sign in with GitHub: tasks

- [x] Read AGENTS.md, the documentation index, the glossary, the
  developer-checklist skill and the GPUI Kit Design and Coding Guides.
- [x] Headless flow crate `subspace-github-auth` (browser with PKCE, device
  flow, scopes, `/user`, revocation, debug loopback overrides).
- [x] Signed-in store and precedence in `subspace-core`; shared `Keychain`
  used by NETT too.
- [x] Consumers: GitHub client, updater, Copilot chat identity.
- [x] Shared `github_sign_in` model and component with a gallery card and
  catalogue entry.
- [x] Settings GitHub accounts group; chat sign-in offer.
- [x] Fixture endpoints, capture wiring, release and packaging secret
  handling, script fixtures.
- [x] Docs: README credentials, settings, agent chat, Actions, installation
  (updater token order, Keychain item), PRs, Actions and Copilot Agents
  guides, UI catalogue.
- [x] Evidence with `make capture`.
- [x] Gates: `make check`, `make interfaces`, `mise run quality`,
  `mise run architecture`.

## Board checklist: PRs, Actions, Copilot Agents

Only their shared GitHub credentials changed; their own integration is
unchanged.

- [x] Identity, registration, artwork, retain: not applicable; unchanged.
- [x] Layout and UI: no board UI changed; the Settings page they share gains
  the GitHub accounts group, built from shared components
  (`github_sign_in`, `action`, `fields`), with a gallery card.
- [x] Settings and configuration: `ConnectionField::GitHubCredentials`
  renders the new group; no board-ID dispatch. Sign-in is outside the Save
  draft and says so.
- [x] Data caching and refresh: not applicable; unchanged.
- [x] Search: not applicable; unchanged.
- [x] Agent tools, MCP and CLI: unchanged; tools use the same credentials,
  so signed-in accounts work there too.
- [x] Chat context: not applicable.
- [x] Notifications and attention: not applicable.
- [x] Help and docs: the three guides now name Sign in with GitHub.
- [x] Communicating with other boards: not applicable.
- [x] Links, logging and diagnostics: the browser opens through `open_url`
  and **Open GitHub** through `links::open`; no token, code or secret is
  logged.
- [x] Testing and evidence: see [delivery](delivery.md).
