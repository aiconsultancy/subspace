# Spec: IRC as a dock

## Layout

- The board is one `DockArea` (`DockSkin`, close buttons on tabs, no dock
  collapse buttons in tab bars).
- Left dock: the networks and buffers, the shared list with its keys (`j`,
  `k`, `Enter`), row emphasis, unread counts and row menus. Resizable.
- Centre: each opened buffer is a `Conversation` panel: header row with a
  divider, transcript, typing line and its own composer and draft. A panel
  draws no dock title bar; a group with several panels shows tabs. A tab's
  title is the channel, nick or network with a badge of unread messages
  (accent when one mentions you). Panels can be dragged between groups and to
  edges to split, resized and closed.
- Right dock: the member list of the focused channel, or a hint for other
  buffers. The member count in a channel's header, the toolbar's
  **Members** and the list's × open and close it. Resizable.
- With no conversation open the centre shows **No conversation open**.

## Opening

- Choosing a buffer in the list (click or `Enter`) shows its panel when it is
  open, else adds it as a tab in the group of the focused conversation, and
  focuses its composer.
- The row menu offers **Open to the right** and **Open below**, which put the
  buffer in a new group beside or under the focused conversation.
- References, notifications, `/query`, a member's row and a new private
  conversation open the same way as a tab.
- The focused conversation is the last one opened or that took keyboard
  focus. The member dock follows it; the hub reads it as messages arrive.

## Persistence

- After every layout change the board stores `irc.dock-layout`: the centre's
  splits (axis and sizes) and tab groups (buffers as `BufferKey`s and the
  displayed tab), and each side dock's width and visibility.
- Once the hub is bound the stored layout is applied. Conversations on
  networks that are no longer configured are dropped, with the tab groups
  and splits they leave empty. The first group's displayed tab takes focus.
  Without a stored layout the first network's **Server** buffer opens.
- Restoring builds panels only; it sends no commands, so it never fights the
  connections. A channel panel restored before its join shows its lines as
  they arrive.

## Removed

- The list board's detail panel host for IRC, the `irc.members-shown`
  preference and the shared hooks added for it (`panel_bar`,
  `trailing_pane`, `PanelBar`, `PanelBody::inset`, `panel_title`).
