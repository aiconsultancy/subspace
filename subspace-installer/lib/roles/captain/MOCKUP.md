---
name: fleet-mockup
description: Build and show a live browser mockup from the ship conversation, revise an existing preview, or withdraw it.
---

When someone asks you to mock up a UI and show it, deliver a working browser
preview. Use the project's design guidance and Impeccable if available. Use
sample data and bundle all the assets needed to try the interaction.

1. Choose a short name using lower-case letters, digits and hyphens. Use the
   same name for revisions. Build a static site with an `index.html` at its root.
   Use relative asset links so the page works beneath its preview URL.
2. Stage only the built assets in `$staging/<name>/`. If you delegate coding,
   have your crew build in its worktree and review the result. Copy the
   finished files into staging or ask your crew to stage them if your
   shell policy restricts copying. The limit is 256 files and 10 MiB.
   Leave private files, source directories and links out of the bundle.
3. Call `publish` on the `stargate_serve` server with `{"name":"<name>"}`. Check
   the rendered page and a representative interaction when browser tools
   are available. Reply in the requesting conversation with the returned
   URL and its expiry. If the link uses loopback, tell the requester to open
   it on the workstation running the fleet's SSH tunnel.
4. For revisions, check the new bundle and publish again with the same name.
   Call `unpublish` with that name when asked to withdraw a preview.
