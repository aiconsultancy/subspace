# Markdown compatibility with GPUI Fast

## Intent

Keep formatted issue descriptions working with GPUI Kit 0.11 and GPUI Fast.

## Specification and delivery

Both native editor elements use GPUI's blanket conversion from `Element` to
`IntoElement`. The first layout that measures a visible span schedules a
follow-up frame, so the editor can cull off-screen blocks in GPUI Fast's
retained frame loop. The tests deliver scheduled frames and account for the
parser's plain whitespace normalisation outside inline code.

The changes come from the editor's upstream GPUI Fast and GPUI Kit 0.11 work.
The [vendor record](../../../../subspace-framework/board-kit/dashboard/vendor/markdown-editor/README.md)
identifies the patches.

## Tasks and checks

- Run the dashboard suite, including the Markdown round-trip, culling and
  caret reveal tests.
- Run the desktop issue editor tests.
