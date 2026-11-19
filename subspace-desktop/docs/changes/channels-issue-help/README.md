# Channels issue navigation documentation

## Intent and specification

Resolve jt's 16 November 2026 fork-review item 5: the Channels guide's Limits
section claimed issue references opened in the browser, while the developer
page described navigation through the issues contract. Describe current code
in both pages; change no navigation behavior.

## Delivery

`spaces/irc/src/issues/follow.rs::open` emits `DashboardEvent::Navigate` with
the `issues@1/current-scope` responder key. `navigation` supplies a scope-bound
URL even without a resolved summary. The shell routes that event to the
answering Space. Without a scope, recognition leaves identifiers as text and
ordinary URLs as browser links. Both the guide and developer page now say so.

The existing headless tests cover reference clicks, Enter, preview tiles and
navigation while a summary is pending. No live services were used.

## Tasks and integration checklist

- [x] Trace the handler and scope fallback in code.
- [x] Correct both documents, including the stale pending-summary browser claim.
- [x] Preserve existing behavior, contract declarations and UI components.
- [x] Space integration checklist: not applicable because only documentation
  changes; no identity, layout, keys, state, tools, settings, search,
  notifications or contract integration changed.
- [x] No new screen or component; screenshots are not required for this
  documentation correction.

See [IRC issue references](../../irc.md#issue-references) and the
[Channels guide](../../../spaces/irc/help.md#limits).
