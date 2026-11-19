# What publishing the Framework exposes

Back to the [documentation index](README.md) and [publishing](publishing.md).

Publishing puts the source of all 24 crates on crates.io, permanently and in
public. This page records what that source contains, as audited on
2026-11-17 for version 0.1.0, and what the owner should decide before the
first publish. Re-run the audit before each release that adds files.

## How it was audited

- `cargo package --list` for every crate (`scripts/check-package.py` runs it
  in `make check`): 1,109 files besides the manifests, all inside their own
  crate folder. No build output, no file over 1 MiB (the largest is the
  vendored Flint bundle, 309 KiB). The only binary images are three
  320×480 plain grey PNG test fixtures in `subspace-shell`
  (`src/app/help_dock/fixtures/`).
- A secret scan of the whole `subspace-framework/` folder with gitleaks
  (`gitleaks dir subspace-framework --redact`, the binary the prek hook
  installs): no leaks found.
- A search of the packaged files for e-mail addresses, `aiconsultancy`,
  `thoughtsun`, `nett`, `workos`, the Apple team id, `Ov23li`, bundle ids,
  internal host names and people's names and handles.
- Re-run on 2026-11-17 after the build-configuration change, over the
  1,183 packaged files: no `aiconsultancy`, `thoughtsun`, `Ov23li`,
  the private tracker workspace or operator's handle. `work app` matches only "Framework apps"
  (`subspace-update`); `ported from` only the credited `gpui-lucide` port
  in `subspace-lucide` (Apache-2.0, in its `NOTICE`).

## Package sizes

Compressed `.crate` sizes, against crates.io's 10 MB limit per crate; 2.25
MiB in total.

| Crate | Files | Size |
| --- | --- | --- |
| `subspace-shell` | 278 | 439 KiB |
| `subspace-dashboard` | 182 | 325 KiB |
| `subspace-agent` | 109 | 198 KiB |
| `subspace-chat` | 71 | 175 KiB |
| `subspace-core` | 103 | 153 KiB |
| `subspace-component-gallery-board` | 77 | 146 KiB |
| `subspace-client-github` | 71 | 110 KiB |
| `subspace-report-issue` | 34 | 96 KiB |
| `subspace-charts` | 17 | 86 KiB |
| `subspace-framework` | 7 | 81 KiB |
| `subspace-search-ui` | 14 | 77 KiB |
| `subspace-lucide` | 9 | 62 KiB |
| `subspace-mcp` | 23 | 56 KiB |
| `subspace-update` | 19 | 54 KiB |
| `subspace-agent-tools` | 24 | 49 KiB |
| `subspace-search` | 42 | 39 KiB |
| `subspace-github-auth` | 15 | 35 KiB |
| `subspace-board-command` | 14 | 35 KiB |
| `subspace-search-onnx` | 13 | 25 KiB |
| `subspace-product` | 19 | 21 KiB |
| `subspace-web` | 11 | 19 KiB |
| `subspace-contracts` | 15 | 18 KiB |
| `subspace-client-test-support` | 7 | 7 KiB |
| `subspace-agent-model` | 7 | 6 KiB |

## Removed before publishing

- **A real person's name and handles in test fixtures.** `subspace-core`'s
  credential tests used a person's full name as GitHub logins
  (`jack-rutherford_shell`, `jrutherford` and case variants) and a search
  test used "Rutherford, Jack"; agent and sign-in tests used the login
  `jack`. They now use the fictional `octo-dev_shell`, `octocat`,
  `Doe, Jane`, `octo-dev` and `octo_shell`. Git history still holds the old
  values, but history is not published.
- **Subspace's identity.** The OAuth App client id, the Report issue
  repository and the organisation's bundle ids and repository in test
  fixtures (see [Resolved](#resolved-before-the-first-publish)).

## Harmless

- **Test fixtures with example identities**: `@example.com`, `.test`,
  `.invalid` addresses, `octo-dev`, `Fixture`, `example-org/example-app`,
  `subspace-networks`, loopback URLs and `localhost`.
- **Third-party material**, each with its licence and a `NOTICE` (see
  [Licence and notices](#licence-and-notices)).

## Resolved before the first publish

- **The issue-report destination.** Report issue is behind the facade's
  `report-issue` feature (not part of `host`), and the repository is the
  app's `ProductBuild::report_issue_repository`; `subspace-client-github`
  takes it from the caller as an `IssueRepository` (`owner/name`). The
  Framework names no repository; its tests use `example-org/example-app`.
- **The GitHub OAuth App.** The Framework holds no client id and reads no
  secret: the app passes its `OAuthClient` as `ProductBuild::github_oauth`.
  Without one, Sign in with GitHub is not offered and no guidance names it.
- **The Linear workspace.** Linear issue links are recognised only for the
  workspace the app supplies on `IssueScope` / `IssueLinkContext`; the
  Framework's tests use the fictional `subspace-networks`.
- **"Ported" origins.** The notes that code was ported from the owner's
  earlier app are gone; it is the owner's original, unpublished work.
- **The persona "Jack".** Comments and tests call the example user Zack.
- **Licence.** Apache-2.0, confirmed by the owner on 17 November 2026.

The last three were fixed in `c36138b`, `ce89fb2`, `8ae45bb` and `51a1b47`.

## For the owner to decide before the first publish

These are product identity, not secrets. Publishing them is safe but makes
them part of the public API and history of the crates.

1. **Crate names.** All 24 crates are named `subspace-*`. Once published, a
   name is taken for good; renaming later means publishing new crates. Check
   that `subspace-framework` and the others are free on crates.io and are
   the names you want (white-labelling would rename them).
2. **Subspace wording and environment names.** User-facing text says
   "Subspace" in places (for example the update and sign-in messages), and
   the Framework reads `SUBSPACE_*` environment variables, among them
   `SUBSPACE_LOG`, `SUBSPACE_MCP_TOKEN`, `SUBSPACE_RELEASE_CHANNEL`,
   `SUBSPACE_GITHUB_TOKEN`, `SUBSPACE_GITHUB_API_URL`,
   `SUBSPACE_GITHUB_OAUTH_URL`, `SUBSPACE_DEFAULT_THEME` (read by the
   product), `SUBSPACE_CAPTURE_*` and `SUBSPACE_NO_PRERELEASE_WARNING`. The
   bundled theme is called "Subspace Channel".
3. **Release tag prefixes in tests.** `subspace-update`'s tests use
   `subspace-desktop-v` tags. The organisation's names (`aiconsultancy`,
   the signing company's name) no longer appear in any packaged file: the
   bundle ids and repositories in `subspace-product`, `subspace-update` and
   the shell's tests are now `com.example.*` and `example-org/example-app`.
4. **NETT references.** `subspace-contracts` documents NETT issue links and
   keys (`TST-827`, `nett-dev`) as an example issue tracker.
5. **`repository` and `homepage`.** The manifests set neither, because the
   repository is private. crates.io will show no source link, and the
   README's relative links to `docs/` will not resolve there. Set both, and
   switch README links to absolute URLs, if the repository becomes public.

## Licence and notices

Every crate ships `LICENSE`, identical to this folder's Apache-2.0 text
(`make check` compares them). Crates that embed third-party material ship a
`NOTICE` that names each file and its licence text, and the package check
fails when a packaged file credits other work without being listed.

| Crate | Third-party material | Licence | Licence text in the crate |
| --- | --- | --- | --- |
| `subspace-charts` | Flint 0.5.1 `chartjs.js` bundle; the Flint authoring guide adapted in `src/guide.md` and `src/lib.rs` | MIT, Microsoft | `vendor/flint-chart-0.5.1/LICENSE` |
| `subspace-shell` | GPUI Component theme catalogue (`themes/gpui-kit/*.json`) | Apache-2.0 | `themes/gpui-kit/LICENSE-APACHE`, `PROVENANCE.md` |
| `subspace-shell` | macOS Classic Dark Remix theme (`themes/macos-classic-dark-remix.json`) | MIT, Jason Lee | `themes/LICENSE-MACOS-CLASSIC` |
| `subspace-chat` | Copilot mark (Primer Octicons) | MIT, GitHub | `assets/LICENSE-OCTICONS` |
| `subspace-chat` | OpenAI mark (Lobe Icons) | MIT, LobeHub | `assets/LICENSE-LOBE-ICONS` |
| `subspace-chat` | Claude, OpenCode and Gemini marks (Simple Icons) | CC0-1.0 | `assets/PROVENANCE.md` |
| `subspace-lucide` | code ported from `gpui-lucide` (suxiaoshao/gupi); the Lucide icons it embeds from `gpui-kit-assets` | Apache-2.0; ISC | `LICENSE`; `LICENSE-LUCIDE` |
| `subspace-component-gallery-board` | Lucide `component.svg` | ISC | `assets/LICENSE-LUCIDE` |

Dependencies that carry their own licence on crates.io need no notice here:
`rquickjs` (QuickJS), `mermaid-rs-renderer` (the canvas diagrams), `gpui-kit`
and the rest. The vendored `markdown` patch is not published.
