//! A surface anchored where it is laid out, such as a card under a focused
//! reference: drawn above its neighbours and kept inside the window.

use gpui_kit::{AnyElement, IntoElement, ParentElement as _, anchored, deferred, px};

/// Keeps a floating surface off the window's edge.
const MARGIN_PX: f32 = 8.;

/// `content` floating at its layout position, above what is drawn later.
// qual:api
pub fn floating(content: impl IntoElement) -> AnyElement {
    let placed = anchored()
        .snap_to_window_with_margin(px(MARGIN_PX))
        .child(content);
    deferred(placed).with_priority(1).into_any_element()
}
