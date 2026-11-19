# Terminal panel: intent

Back to the [change records](../README.md).

The owner asked to bring back the libghostty terminal, not as a Space but as
a Framework feature shown by the shell across every Space, the way the chat
panel is:

- It docks on the left, the right or the bottom, and pushes the Space over
  or up rather than covering it.
- For the bottom position there are two layouts, both available as an
  option: the terminal spans the full window width, or the chat keeps the
  full window height and pushes the terminal in. The default is that the
  chat pushes it in.
- A maximize ("zoom") button makes it fill the whole window body, covering
  the Space and the chat, until it is restored.
- Repositioning is by menu and buttons first; dragging the panel to an edge
  comes as the next layer.
- Ctrl+` shows and hides it on every platform.
- Every button that repositions the terminal (left, bottom, right, full
  width, maximize) is a glyph button. Its text can be shown through a
  toggle; by default the text is off.
- For now it ships in debug builds only. Release builds stay Zig-free.

Today the terminal is the Terminal Space (`spaces/terminal`), behind the
`terminal` Cargo feature, which release builds never enable. The shell
already has unused per-board dock hooks (`Dashboard::workspace_dock`,
`workspace_dock_minimum`, `workspace_maximized`,
`DashboardEvent::ResizeWorkspaceDock`) and a `terminal.placement` config key
that no Space reads.
