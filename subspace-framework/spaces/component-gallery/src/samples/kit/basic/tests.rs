//! Every basic builder, called directly, captions each of its states.

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;

use super::{ITEMS, choices, controls, display, messages, status, text};
use crate::samples::tests::{captions, every_state_is_captioned};
use crate::tests::open_gallery;
use crate::view::named;

#[gpui_kit::test]
fn basic_builders_caption_every_state(cx: &mut TestAppContext) {
    let items = captions(cx, |g, t, w, cx| {
        vec![
            status::accordions(g, t, w, cx),
            display::alerts(g, t, w, cx),
            messages::attachments(g, t, w, cx),
            messages::avatars(g, t, w, cx),
            display::badges(g, t, w, cx),
            display::breadcrumbs(g, t, w, cx),
            messages::bubbles(g, t, w, cx),
            controls::buttons(g, t, w, cx),
            controls::checkboxes(g, t, w, cx),
            controls::clipboards(g, t, w, cx),
            status::collapsibles(g, t, w, cx),
            controls::dropdown_buttons(g, t, w, cx),
            display::empties(g, t, w, cx),
            display::icons(g, t, w, cx),
            display::images(g, t, w, cx),
            display::keys(g, t, w, cx),
            text::labels(g, t, w, cx),
            text::links(g, t, w, cx),
            messages::markers(g, t, w, cx),
            messages::messages(g, t, w, cx),
            messages::message_scrollers(g, t, w, cx),
            controls::paginations(g, t, w, cx),
            status::progress(g, t, w, cx),
            messages::questionnaires(g, t, w, cx),
            choices::radios(g, t, w, cx),
            choices::ratings(g, t, w, cx),
            text::separators(g, t, w, cx),
            text::shimmers(g, t, w, cx),
            text::skeletons(g, t, w, cx),
            choices::sliders(g, t, w, cx),
            status::spinners(g, t, w, cx),
            choices::steppers(g, t, w, cx),
            choices::switches(g, t, w, cx),
            text::tags(g, t, w, cx),
            text::text_views(g, t, w, cx),
            choices::toggles(g, t, w, cx),
            text::tooltips(g, t, w, cx),
        ]
    });
    assert_eq!(items.len(), ITEMS.len());
    assert!(every_state_is_captioned(&items));
}

#[gpui_kit::test]
fn clicking_the_checkbox_sample_changes_its_value(cx: &mut TestAppContext) {
    let (gallery, mut cx) = open_gallery(cx);
    let name = "gpui_kit::component::checkbox::Checkbox";
    let checked = |cx: &mut gpui_kit::VisualTestContext| {
        cx.update(|_, cx| {
            gallery
                .read(cx)
                .retained
                .values
                .read(cx)
                .flag("kit-checkbox", true)
        })
    };
    cx.update(|window, cx| {
        gallery.update(cx, |gallery, cx| gallery.reveal(name, window, cx));
        window.render_frame(cx);
        assert!(window.try_find(named("gallery-item", name)).is_some());
        window.click("gallery-kit:checkbox", cx);
    });
    assert!(!checked(&mut cx));
}
