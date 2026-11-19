# Spec: IRC conversation header and member column

- A channel or private conversation's header is the detail panel's bar: back,
  the name in the heading font, then `<network> · <topic>` in muted text on
  the same row, cut with an ellipsis and shown whole in a tooltip, then the
  member count (channels), previous, next, Add to chat and close. The bar's
  `line` border is the divider. The panel draws no second title.
- The transcript fills the panel from that divider down to the composer, with
  no padding above it, so nothing shows between the header and the first
  visible row.
- The member list is a full-height column to the right of the panel with a
  `line` boundary, a bar-height header **Members · N** with a close button
  (Hide members), and its own scrolling. Its top is the header's top.
- The member count and the toolbar's **Members** show or hide it; the close
  button hides it. The choice is stored as `irc.members-shown`.
- The **Server** panel keeps its title, facts and transcript.
- Shared: `ListDashboard::panel_bar` with `panel::PanelBar` and
  `PanelActions::with_bar`, `ListDashboard::trailing_pane`, and
  `PanelBody::inset`. `ListDashboard::panel_title` from the previous change
  is removed.
