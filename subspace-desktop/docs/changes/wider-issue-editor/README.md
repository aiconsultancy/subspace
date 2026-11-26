# Wider issue editor

The shared issue form has a wider layout and a taller description field.
The field wraps long lines and the full form scrolls when the window is short.

Markdown mode highlights syntax. Plain text mode shows the same editable
draft without highlighting. Switching modes keeps the text, mentions and
undo history in one editor. Tab scrolls the next focused field into view.

The native issue editing test switches to Plain text, edits a description,
tabs out, returns to Markdown and checks the exact saved description.

## Captures

The native fixture uses the Twilight Two theme.

- [Before](evidence/before.png)
- [Markdown](evidence/markdown.png)
- [Plain text](evidence/plain.png)
