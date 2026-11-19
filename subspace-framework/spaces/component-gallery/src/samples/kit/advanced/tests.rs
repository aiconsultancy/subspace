//! Every advanced builder, called directly, captions each of its states.

use gpui_kit::TestAppContext;

use super::{ITEMS, charts, data, dock, menus, tables, views};
use crate::samples::tests::{captions, every_state_is_captioned};

#[gpui_kit::test]
fn advanced_builders_caption_every_state(cx: &mut TestAppContext) {
    let items = captions(cx, |g, t, w, cx| {
        vec![
            data::diffs(g, t, w, cx),
            data::calendars(g, t, w, cx),
            data::carousels(g, t, w, cx),
            data::commands(g, t, w, cx),
            charts::charts(g, t, w, cx),
            charts::plots(g, t, w, cx),
            views::lists(g, t, w, cx),
            menus::menus(g, t, w, cx),
            menus::native_menus(g, t, w, cx),
            views::settings(g, t, w, cx),
            tables::data_tables(g, t, w, cx),
            tables::tables(g, t, w, cx),
            dock::docks(g, t, w, cx),
            views::grid_layouts(g, t, w, cx),
            menus::tabs(g, t, w, cx),
            views::trees(g, t, w, cx),
            views::virtual_lists(g, t, w, cx),
        ]
    });
    assert_eq!(items.len(), ITEMS.len());
    assert!(every_state_is_captioned(&items));
}
