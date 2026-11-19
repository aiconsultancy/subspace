# Intent: IRC as a dock

The owner asked for the IRC board to use GPUI Kit's dock "so that we can
move windows around, have them split above and below, open multiple windows
like direct message windows and the channel windows, and have the menus on
the left and the right". Panels stay inside the app window; no separate OS
windows.

The goal is a chat client layout built from Kit's `DockArea`: the buffer
list as a left dock, each channel, private conversation and server buffer as
its own panel that can be tabbed, split side by side or above and below,
moved and closed, and the member list of the focused channel as a right
dock, with the arrangement remembered between launches.
