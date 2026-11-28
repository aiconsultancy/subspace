# Channels attachments and Ship refresh, and issue titles

## Intent

Make it possible to attach several common web files and documents to a draft
by dropping them on the composer. Keep the Ship tab steady as fleet data
arrives or refreshes. Keep long issue titles inside the reading column.

## Behaviour

- The composer queues multiple dropped files in order above the draft.
  External file drags restore pointer input after typing. Dropping and using
  the paperclip share the same file preparation and limits. HTML, CSS, SVG,
  JSON and other supported UTF-8 source files keep their filenames and open
  as text. WebP still has an image preview, and PPTX is a downloadable file.
- The Ship tab waits for its first crew read before showing the sections
  below it. It keeps the last complete read during a brief interruption and
  labels it as last known after five seconds. A successful read updates the
  tab in place. Permission failures and ships leaving the roster show their
  own status immediately.
- Issue titles wrap within the available reading width, beside the
  Properties inspector. Selection, copying and find-in-page use the full
  title.

## Verification

- Native tests cover a drop after typing, mixed attachment order and MIME
  types, the initial Ship read and recovery, a partial fleet poll, and issue
  title bounds with Properties open.
- The loopback API test uploads an HTM file, downloads it under its original
  name and reconstructs its contents with the agent text tool.
- The captures use the isolated macOS fixture build in Twilight Two. A
  delayed fixture crew read exercises the Ship tab's first load. The issue
  fixture uses the same long title and window dimensions in both builds.

## Screenshots

[Attachments before](evidence/attachments-before.png) ·
[Attachments after](evidence/attachments-after.png)

[Ship loading before](evidence/ship-loading-before.png) ·
[Ship loading after](evidence/ship-loading-after.png) ·
[Ship loaded](evidence/ship-loaded-after.png)

[Issue title before](evidence/issue-title-before.png) ·
[Issue title after](evidence/issue-title-after.png)
