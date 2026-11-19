# Intent: NETT issue workspace

Operators work in Subspace's IRC board, where people mention NETT issues such
as `TST-827`. Today they leave the app and open NETT in a browser to read one.
The Issues and Channels initiative adds a read-only issue workspace inside Subspace so an
operator can read an issue, its tracker context, comments, images, related
decisions and activity without leaving the app, and open an IRC reference
straight into it.

NETT issue client core (slice 1 of the issue workspace implementation, recorded in
[nett-issue-client-core](../nett-issue-client-core/) and merged at `e8757cf`) supplies the NETT
client, WorkOS device sign-in, keychain storage, API profiles and reference
recognition. This record covers everything after it:
the Issues board, the issue view, IRC references and the reads behind them.

## Owner direction (15 November 2026)

- **NETT's desktop layout.** The issue list and the issue view look and sit
  where they do in NETT on desktop, so people know where to look, drawn with
  Subspace's shell, tokens and components.
- **Comments always visible; other activity folded** until opened.
- **Read only.** No writes, no costs and no documents. Links to NETT pages
  other than issues are not followed; links to other sites open in the
  system browser.
- **NETT dev first** (`https://nett.example.com/api`), with WorkOS device sign-in
  as `indractl` does it, and separate sign-in per API profile. Production
  follows in the production profile rollout.
- **First milestone:** issues load from NETT dev after device sign-in, and an
  IRC reference opens the issue. The tooltip is a nice-to-have.

The approved design is [design.md](design.md), with the captain's decisions
in [design-approval.md](design-approval.md).

## Out of scope

- Any write to NETT: comments, edits, state changes, subscriptions, uploads.
- Costs, Documents, Cycles, Views, the Board display and saved views.
- A decision reader, Save image, and downloading attachments.
- NETT's realtime hub and automatic polling.
- Issue content in `state.json`, the board cache, the search index, agent
  tools or chat context, or anywhere on disk.
- The standalone Subspace API profile until Kirk confirms its address,
  contract and sign-in (approval decision 4).
- Production NETT (the production profile rollout) and the dev edge itself (the development access update, the edge access follow-up).
