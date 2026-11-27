# Markdown issue descriptions

## Intent

Let people read and edit an issue description as formatted prose in the same
field. Keep a highlighted Source mode for exact Markdown edits.

## Specification

New issue, Edit issue and Edit description open the same field with Markdown
selected. The formatted field has a caret and formatting controls. Source
uses the existing highlighted native editor. Both modes read and save the
same Markdown draft. Opening a description or switching modes alone preserves
the original bytes. Undoing a formatted edit back to its imported baseline
restores that exact source. A formatted edit serialises its result as canonical
Markdown. When someone changes text in one mode, the other loads the updated
draft on entry and starts a fresh local undo history. Images show placeholders
during editing and keep their Markdown source. Tab and Shift+Tab navigate the
form from the editing area. The issue form owns validation, write access,
conflict recovery and submission.

## Delivery

The Framework's `markdown_editor::MarkdownField` composes the GPUI block editor
and the highlighted source editor. Its interactive Component gallery card
uses the same control as Issues.

## Tasks and checks

- Wire the shared field into the issue form and update the help.
- Check mode switching, exact unchanged Markdown, direct formatted edits,
  locked fields, undo and the issue save payload with native interaction tests.
- Compare the before and after issue form and the Component gallery card in
  the Twilight Two capture fixture.

## Board checklist

The existing Issues and Component gallery registrations, workspace layout,
text edges, docks, toolbar, keyboard commands, status, palette, dialogs,
empty and failure states, help links and navigation remain in use. Issues uses
the same API write boundary, permissions, logs and conflict flow. The new
shared field is listed in the Framework catalogue and its gallery card is
interactive. The change introduces no new settings, storage, refresh,
search, indexing, tools, contracts, notifications or external navigation.

## Capture evidence

- [Before](evidence/before.png) shows the syntax-coloured box on New issue.
- [New issue](evidence/markdown-empty.png) starts with the formatted editor
  and its formatting controls.
- [Markdown](evidence/markdown.png) and [Source](evidence/source.png) show
  the same draft after switching modes.
- The interactive Component gallery card shows a heading, bold text and a
  list in [Markdown](evidence/gallery.png) and [Source](evidence/gallery-source.png).
