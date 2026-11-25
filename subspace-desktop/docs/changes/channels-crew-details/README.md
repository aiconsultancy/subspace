# Channels crew details

## Intent

SUB-68 adds a crew view to the Ship tab so that people can see what a
crew member is doing across projects without leaving the conversation.

## Delivery

Select a crew member in the Ship tab. The inspector shows their state,
model and current task, and each project's Git state and pull request.
**Back to Ship** returns to the crew list. **Open PR** uses the existing
pull request navigation. Branch diffs and activity are separate work.

The view reads the crew detail endpoint in the background and refreshes it
when the member changes on the live roster. It keeps the last full detail
while a refresh runs, using the roster's newer task text until the full
response arrives. It falls back to roster data when detail is unavailable;
the current roster also decides whether the member is still on the ship.
Task text has no length limit. Control and bidirectional formatting codes
are written out as code points, except line breaks and tabs.

## Verification

- The Fleet client tests cover the read, missing route and response identity.
- The Channels tests cover roster changes, missing Git state, no checkout,
  and pull request display. Ship tab tests check accessible names, keyboard
  focus into and out of the detail view, and clearing the view on a buffer change.
- Before marking the pull request ready, capture the Ship tab and crew view
  in Twilight Two using the fictional Fleet and IRC fixtures described in
  [Desktop capture](../../capture.md).

## Space checklist

This extends the existing Channels inspector, without adding a board,
changing its zones or adding a command, setting or board-to-board role.
The inspector uses shared sections and action components; the crew row
uses a GPUI Kit button. The existing [Channels guide](../../irc.md) describes
how to open and leave the view.
