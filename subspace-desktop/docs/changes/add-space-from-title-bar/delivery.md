# Delivery: add a Space from the title bar

See the [spec](spec.md) and [tasks](tasks.md).

## State on 21 November 2026

Built on the same branch as the
[workspace shell](../workspace-shell/delivery.md), and its tests pass in the
host app suite (`subspace-shell`) listed there, from plain
`cargo test --workspace --locked --no-fail-fast` on the working tree: the
pure logic in `app/add_space.rs` and the runtime tests in
`app/space_menu_tests/add_space.rs`.

On the final working tree, on 21 November 2026, `make check` passed in the
Framework and in the desktop and `mise run quality` passed in both.

## Passed

- [x] `make check` in the Framework, then in the desktop (21 November 2026,
      exit 0 in both).
- [x] `mise run quality` in both projects (21 November 2026): the Framework
      with no new findings against the baseline, the desktop exit 0.

## Not yet done

- [ ] Captures in Twilight Two under `evidence/`:
  - the title bar with the plus after the last tab;
  - the open menu with Spaces switched off;
  - the menu with every Space on;
  - the plus in the chevron menu in a narrow window;
  - the plus at the end of the navigation rail;
  - the notice after closing a Space and after adding one.
- [ ] Both model reviews, Claude Opus 5.5 and GPT-6.2 Sol at high effort,
      clear the final head. Not run.

The Add a Space menu has not been captured. The plus after the last Space tab shows in
the workspace shell's captures, such as
[the Issues list](../workspace-shell/evidence/issues.png), but closed.
