# Delivery: the standard right-click menu

See the [intent](intent.md), [spec](spec.md) and [tasks](tasks.md).

## State on 21 November 2026

Built on the same branch as the [workspace shell](../workspace-shell/delivery.md),
and its tests pass in the suites listed there, from plain
`cargo test --workspace --locked --no-fail-fast` on the working tree: the
Framework dashboard (`row_menu`'s own tests: group order, separators,
disabled reasons, the danger glyph, the standard commands of a row, a header
and the panel), the host app (the Space tab's menu), Channels (each row's
removal) and Pull requests (the menu at run time).

On the final working tree, on 21 November 2026, `make check` passed in the
Framework and in the desktop and `mise run quality` passed in both.

## Passed

- [x] `make check` in the Framework, then in the desktop (21 November 2026,
      exit 0 in both).
- [x] `mise run quality` in both projects (21 November 2026): the Framework
      with no new findings against the baseline, the desktop exit 0.

## Not yet done

- [ ] Captures in Twilight Two under `evidence/`: a list row's menu, a group
      header's menu, a Remove command and the Space tab's menu. No
      right-click menu has been captured.
- [ ] Both model reviews, Claude Opus 5.5 and GPT-6.2 Sol at high effort,
      clear the final head. Not run.

The captures and the model reviews have not been done.

## Known gaps

- **Remove labels are not red.** Only the glyph of a Remove command takes
  the danger colour. GPUI Kit's native menu cannot colour a label without
  losing it as the item's accessible name, so the label keeps the menu's
  text colour.
- **Pull requests' panel menu.** The old detail panel had its own
  right-click menu; the inspector that replaced it has none.
- **Copilot Agents' Copy ID.** Its row menu no longer offers **Copy ID**.
