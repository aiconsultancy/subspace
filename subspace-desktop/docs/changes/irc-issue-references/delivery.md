# Delivery: IRC issue references

Back to the [change records](../README.md). [Intent](intent.md),
[spec](spec.md), [tasks](tasks.md).

## Scope

Phase 4 of the [board-contracts plan](../board-contracts/tasks.md), on
`feat/subspace-desktop` after `1323d60` (the recogniser move into
`subspace_contracts::issues::recognition`). No board answers the `issues`
contract yet: the Issues board does in phase 3. Tests use a fixture responder
board routed by the real broker. The screenshots use a stand-in responder
that only debug capture builds contain.

## What changed

| Area | Files |
| --- | --- |
| Contract adoption and resolving | `spaces/irc/src/issues.rs`, `issues/follow.rs`, `issues/tooltip.rs`, `issues/tests.rs`; `hub.rs` (the `issues` field, `attach` at start); `lib.rs` (`contracts`, `keys`, `bind_keys`); `Cargo.toml` (`subspace-contracts`) |
| Recognition | `markup.rs` (`Mentions`, `Ref::Issue`, `classify` split into steps, `issues`), `markup/issue.rs`, `markup/issue_tests.rs`, `markup/link.rs`, `markup/markdown/{inline,spans,lines,parsed}.rs`, `markup/markdown.rs` |
| Looks and tooltip | `style.rs`, `style/marks.rs` (moved `with_refs` and the marking helpers, new `with_unavailable`, `with_focus`), `text.rs` (`Opens`, `RefHandlers`, tooltip, unavailable and focused looks), `transcript.rs`, `transcript/line.rs`, `transcript/line/marks.rs` |
| Keyboard | `conversation.rs`, `conversation/keys.rs`, `transcript/focus.rs`; `dock/open.rs` (opening an issue) |
| Tests | `tests/issues.rs`, `tests/focus.rs`, `style/tests.rs`, renamed `Mentions` in the markup tests |
| Board-kit | `anchored::floating` (`anchored/floating.rs`); stale `qual:api` markers on `Declaration::publishes` and `responds` |
| Contracts crate | stale `qual:api` markers in `issues.rs` |
| Host (capture only) | `capture/issues.rs` (stand-in responder), `capture.rs`, `registry/background.rs`; `Cargo.toml` (`subspace-contracts` optional, enabled by `capture`) |
| Docs | `docs/irc.md` (Issue references), `spaces/irc/help.md`, `docs/board-integration.md` (Declared contracts, Contracts, audit, IRC gaps), `docs/issues.md`, `docs/ui-components.md`, `docs/changes/board-contracts/tasks.md`, this record |

## Checks

- `make check`: passed.
- `make interfaces`: passed.
- `mise run quality`: 0 findings and 0 violations against the baseline. The
  first run found the new transcript focus type named `Cursor` (rustqual took
  `std::io::Cursor::new` elsewhere for project calls), a struct and two modules
  over their size, a nested `classify` and an `impl Hub` outside its module.
  All were fixed in the code, not suppressed.
- `cargo clippy -p subspace --features subspace-shell/capture -- -D warnings`:
  passed.
- New tests: 10 in `issues::tests`, 5 in `markup::issue_tests`, 2 in
  `style::tests`, 2 in `transcript::focus`, 5 in `tests::issues` (the real
  broker with a fixture responder), 1 in `tests::focus`, 1 in the host's
  `capture::issues`. The recogniser tests moved with `1323d60`.

## Screenshots

Cupertino Dark (theme unset), the debug capture build
(`cargo build -p subspace --features subspace-shell/capture`), an isolated
`HOME`, the local Ergo server (`scripts/irc-test-server.sh`, stopped
afterwards) with the fictional people theo and alice posting to `#ops-deck`
on the network `Harbor Lab`, Subspace signed in as mira. The stand-in
responder (`SUBSPACE_CAPTURE_ISSUES=1`) answers as profile `NETT dev` on
`https://nett.example.test` with keys `TST` and `OPS`: `TST-1612` and
`TST-1588` available, `TST-1700` pending and every other identifier
unavailable. All issues, people and addresses are fictional.

References: TST-1612, TST-1588 and TST-1700 on the accent tint, OPS-42 muted and underlined as unavailable, a NETT issue link as a reference, and an ordinary link unchanged.

The keyboard's TST-1612 with its card: identifier, title, the In Progress glyph, the assignee and the profile.

The keyboard's OPS-42, unavailable: "OPS-42 isn't available with NETT dev."

The keyboard's TST-1700 while its answer is pending: "Loading TST-1700…"

Keyboard focus on the newest message, with its link focused.

With no board answering the contract, identifiers are plain text and links are ordinary links.

The last screenshot also serves as *before*: without a responder, IRC renders
messages as it did before this change. There was no separate `origin/main`
build, because a second target directory was ruled out.

Not captured:

- **Hover tooltips**: the capture build's pointer steps do not reach GPUI's
  hover in a background window, as earlier IRC records found. The hover
  tooltip is the same card (`issues::tooltip`) as the keyboard's, and
  `text::formatted` attaches it.
- **Signed-out and error tooltips**: the stand-in has no such answers. The
  wording is tested in `issues::tests`.
- **Opening in the Issues board** and `error-not-found` from IRC: these need
  `BoardNavigation`. IRC opens the issue's page in the browser, as
  `tests::focus` checks.
