# Choose and drop files in Channels

## Intent

The paperclip needs to open a file picker. Converting a draft to a Markdown
file needs its own action. Dropping a file on the message box should leave
the draft in place and queue the file above the box.

## Behaviour

**Attach files…** opens the system picker for one or more files, even if the
message box is empty. The drop target covers the message box itself.
Picked, dropped and pasted files enter the same tray in intake order and
are uploaded when the user sends the message. **Attach draft as Markdown**
appears while there is text to convert.

The controls use the existing Button, Icon Button and Attachment components.
The Channels board, its configuration and the IRC send protocol keep their
existing contracts.

## Delivery and checks

- The composer tests exercise picker cancellation, ordered multiple selection
  and a platform file-drop event over a focused message box. They check that
  no file is uploaded during intake and that the draft survives the drop.
- The attachment tests cover conversion to Markdown and the existing
  single-message delivery path.
- In the isolated macOS build, the native picker added two synthetic files
  to an empty composer. The capture also dropped two files onto a written
  draft in a fictional IRC channel. Both appeared above the intact draft.

[Before](evidence/paperclip-before.png) ·
[Attach files](evidence/paperclip-after.png) ·
[File in the tray with the draft](evidence/drop-after.png)
