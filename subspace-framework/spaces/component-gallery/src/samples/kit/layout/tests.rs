//! Every layout builder, called directly, captions each of its states.

use gpui_kit::TestAppContext;

use super::{ITEMS, overlays, regions};
use crate::samples::tests::{captions, every_state_is_captioned};

#[gpui_kit::test]
fn layout_builders_caption_every_state(cx: &mut TestAppContext) {
    let items = captions(cx, |g, t, w, cx| {
        vec![
            regions::description_lists(g, t, w, cx),
            regions::group_boxes(g, t, w, cx),
            overlays::roots(g, t, w, cx),
            overlays::themes(g, t, w, cx),
            overlays::dialogs(g, t, w, cx),
            overlays::alert_dialogs(g, t, w, cx),
            overlays::notifications(g, t, w, cx),
            overlays::popovers(g, t, w, cx),
            overlays::hover_cards(g, t, w, cx),
            regions::resizables(g, t, w, cx),
            regions::scrollables(g, t, w, cx),
            overlays::sheets(g, t, w, cx),
            regions::sidebars(g, t, w, cx),
            regions::status_bars(g, t, w, cx),
            regions::toolbars(g, t, w, cx),
            regions::title_bars(g, t, w, cx),
            regions::focus_traps(g, t, w, cx),
        ]
    });
    assert_eq!(items.len(), ITEMS.len());
    assert!(every_state_is_captioned(&items));
}
