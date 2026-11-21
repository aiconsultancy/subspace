# Intent: the standard right-click menu

Back to [change records](../README.md).

Each right-click menu was put together where it opened. The shared list's
row menu offered **Open details**, the copies and the chat, and a Space
appended its own items after them in the order it chose; the Space tab's
menu was built on its own. The same kind of command could sit in a different
place, with different words, from one menu to the next.

The owner designed one menu for every right-click on 20 November 2026, while
the [workspace shell](../workspace-shell/intent.md) was being built. That
record deferred it as a separate change; this is that change.

## The agreed design

As the design guide's
[Context menus](../../../../subspace-framework/docs/design-guide.md#context-menus)
records it:

- **One order everywhere.** Every menu is built from the same seven groups
  in the same order, so a kind of command is always in the same place.
- **Right-click acts on what is highlighted.** The row selects first.
- **Say why not.** A command that cannot run now stays, disabled, with the
  reason.
- **Removal comes late and is marked** in the theme's danger colour.
- **Never the only way.** Each command is also on the toolbar, in the
  inspector or in the command palette.

See the [spec](spec.md), [tasks](tasks.md) and [delivery](delivery.md).
