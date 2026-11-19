# Intent: flat controls

After [macOS controls](../macos-controls/intent.md) put toolbar buttons on
Liquid Glass, the owner asked for flat controls, in five steps:

> Buttons in the status bar should always appear flat. The Report issue
> button is now wrong there; the clickable version string that shows the
> About is correct.

> These glyph buttons should be flat by default.

The second report showed the global toolbar's glyph buttons (Chat, Search,
Commands, Reload, Wrap, Theme designer, Settings, Help, About, Report issue)
in Cupertino Dark, each a glass capsule with a visible rim at rest.

> Can this be configuration based per theme?

> We also want flat on hover.

> Show a highlight bg colour on hover. For toggle buttons there should be a
> change to show they are toggled too.

> Status bar buttons should always be flat / flat no glass states.

So:

- Glyph toolbar buttons are flat by default in every theme: no fill, rim or
  outline at rest, a subtle highlight fill under the pointer and a stronger
  one while pressed.
- A latched (toggled) button shows that it is on without hovering, with more
  than colour: a persistent fill and a changed glyph.
- A theme may opt into glass at rest, on hover, or both.
- The status bar is flat in every theme and state; its Report issue matches
  the version button.
