# GPUI Kit Markdown editor

The dashboard crate bundles the Apache-2.0
[GPUI Kit Markdown editor 0.2.0](https://github.com/wabzqem/gpui-kit-markdown-editor/tree/v0.2.0)
against GPUI Kit 0.9.1. Its source and icons come from the published archive
with SHA-256
`e9ba0ca3c491247af574cde025f2f23ad0b24cf29b1b034ac516deef8cc8513f`.
The [upstream licence](LICENSE-APACHE) and
[Lucide icon licence](assets/icons/LICENSE-LUCIDE) accompany it.

The changes in `src/direct.rs`, `src/frame.rs` and `src/tests/view.rs` come from the upstream
[GPUI Kit compatibility patch](https://github.com/wabzqem/gpui-kit-markdown-editor/commit/ccaf935a709bee8b910e40aad4fd3a01c5270293).
They initialise the task marker span and use GPUI Kit's aligned table geometry
for caret placement and hit testing. GPUI Kit 0.9.1 already has those APIs.
Test content and links use synthetic fixtures, including the sample under
`tests/fixtures`. Paths rooted at the upstream crate now resolve in the
dashboard’s private module.
Subspace escapes GFM table separators and literal punctuation in image labels
during export. Duplicate link definitions follow CommonMark's first match.
The host owns the command-palette and form navigation shortcuts.

Update the dependency to the published editor when the Framework moves to a
compatible GPUI Kit release. Spaces use `dashboard::markdown_editor` through
the Framework.
