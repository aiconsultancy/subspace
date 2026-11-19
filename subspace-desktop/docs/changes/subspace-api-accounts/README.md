# Subspace API, accounts and fleets

Back to the [change records](../README.md).

## Intent

Bring the generic features a downstream fork built on this repository back
upstream, without anything specific to that organisation, so that the
desktop runs against the open-source stack in this repository (Subspace API,
Ergo IRC and fleets) and a downstream app can be composed from the published
Framework plus its own private crates.

## What changed

- **Issues on Subspace API.** `clients/subspace-api` replaces the NETT client:
  projects, initiatives, issue lists and groups, the issue page with editing,
  creation, moves, summaries, comments, images and attachments, decisions,
  linked pull requests, temporary chat uploads, and live updates over
  server-sent events with a polling fallback. The offline fixture
  `scripts/subspace_api_test_server.py` replaces the NETT fixture.
- **Accounts.** The Framework gains a provider-neutral account
  (`subspace_framework::account::Provider`, `shell::AccountRegistration`,
  `shell::account_session`) with **Settings > Account**, silent restoration
  and renewal. It ships Microsoft Entra and WorkOS AuthKit providers (system
  browser, PKCE, loopback redirect). The app adds a `dev` provider for a local
  API. The build config and run-time variables select the provider and API
  URL; nothing names a real tenant or client.
- **Channels.** An initiative picker shared with Issues, initiative channel
  sets, fleet-aware connection choice (an initiative's fleet IRC server from
  Subspace API), several attachments per message with thumbnails, pull request
  chips and hover cards, issue tiles and summaries, wrapped paragraphs and
  batched multiline sends, link-click fixes, and PR-merge and deployment
  notices for captains.
- **Other Spaces and the shell.** List Space headers and spacing, Actions
  filters, loading and background refresh, PR close and navigation, resizable
  attachment viewers, issue search fallback, back navigation to the origin,
  find in page and text menus, About credits and a title prefix hook, native
  Linux capture, and the release repository in the product descriptor.

## Deliberately left out

- The downstream organisation's identity registration, API host, branding,
  logo, release repository, release and packaging workflows, and its
  website Space with an embedded browser session.
- The downstream fork's patched copy of GPUI Kit's `gpui-base`. The Framework
  is published to crates.io and must build with the published crate, so find
  in page shows plain-text matches as styled text, the text menu cannot
  offer **Copy link** for a link under a secondary click, and a link click
  right after a selection ended outside the window can be ignored, until GPUI
  Kit supplies those APIs and the selection-latch fix. The Channels tests for
  that case are kept, ignored, with that reason.

## Delivery

Checked with the Framework's and the app's `make check` gates and
`mise run quality`; the commands and results are in the branch's report.
Screenshots of the changed UI still need capturing with `make capture` before
a pull request.

## Review fixes

- Dropping an abandoned browser sign-in releases its callback listener.
  WorkOS credentials are scoped to the configured origin, and refresh keeps
  the signed-in organisation.
- Development token renewal keeps explicit sign-out in effect. Offline
  fixtures use their own token for both REST and the issue update stream.
- Channels accepts space- and comma-separated initiative channels. Captain
  notices require a joined channel on the issue initiative's IRC endpoint.
- Token leases are refused while an account switch or sign-out runs, and a
  committed account gets a new generation, so a client created during the
  switch never serves either account's token as current. A refresh that goes
  stale still keeps its rotated refresh credential.
- An initiative whose assigned fleet cannot be read keeps its autojoin off
  instead of using the default IRC server, and the memberships are read again
  with backoff.
