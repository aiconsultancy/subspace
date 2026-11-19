# Selectable text in find in page

## Intent

Keep plain values selectable while find in page marks matches. Offer **Copy
link** in the text menu when the pointer is over a Markdown link, including
when the label is selected.

## Specification

Finding a match retains native selection and copy. Moving through the
matches reveals the visual line containing the active one. A secondary
click over a rendered link exposes its destination to the enclosing text
menu and preserves the selection. The next primary press can clear a
selection even if the previous release never reached the selection layer.

## Delivery

The desktop opts into the [vendored GPUI Kit text support](../../../vendor/gpui-base/SUBSPACE-PATCH.md).
The Component gallery's `find_in_page::surface` sample includes a literal
match and a Markdown link. The Framework feature is opt-in so normal
registry builds can continue to use the published GPUI Kit.

The backport follows the merged [selectable-text reveal](https://github.com/longbridge/gpui-kit/pull/4052)
and [linked-image click handling](https://github.com/longbridge/gpui-kit/pull/4053).
The text menu waits for the secondary `ClickEvent` from
`TextView::on_link_click` before it shows its items. The local selection
compatibility change delivers that event even if the link is selected.
The [press latch fix](https://github.com/longbridge/gpui-kit/pull/4054) is open
upstream and remains in the backport.

## Verification

The dashboard tests exercise copying a selected link across a redraw
between press and release and selecting a plain-text match while find is
open. The vendored crate covers reveal geometry and deduplication,
auxiliary clicks on formatted links and linked images, and selection
recovery after a missed release.
