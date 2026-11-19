# Intent: close a Space from its tab

The owner asked for a right-click menu on the Space tabs in the switcher,
with an option to close that Space. Until now the only way to close one was
its **Enabled** switch in Settings, several clicks away.

The owner's requirements:

- Closing uses the same effect as turning the Space off in Settings: one way
  of hiding a Space, not two.
- Closing the current Space moves to a neighbour (the tab before it, or the
  next when it was first) without losing focus handling.
- The last Space can't be closed; the item is disabled, with a reason where
  the menu supports one.
- A closed Space comes back from Settings exactly as before. If the app has
  a notice pattern, it says where to reopen the Space.
- A second item that opens the Space's own Settings page, if it is cheap.
- Keyboard access to the menu, if the tabs take focus and GPUI Kit supports
  opening a context menu from the keyboard.

Back to the [change record](spec.md).
