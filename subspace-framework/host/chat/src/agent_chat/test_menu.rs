//! Native popup items expose numeric IDs and accessibility labels in Kit 0.9.1.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{App, Window};

pub(super) fn item_index(window: &mut Window, label: &str) -> usize {
    let menu = window.within("popup-menu");
    (0usize..64)
        .find(|ix| {
            menu.try_find(*ix)
                .is_some_and(|item| item.visible() && item.label() == Some(label))
        })
        .expect("visible native popup item")
}

pub(super) fn click(window: &mut Window, label: &str, cx: &mut App) {
    let ix = item_index(window, label);
    window.within("popup-menu").click(ix, cx);
}
