//! Every item builder, called directly: each returns at least one sample, and
//! every sample has a state caption. The catalogue refers to the builders as
//! function pointers, so these calls also keep each builder in the test graph.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Context, Role, SharedString, TestAppContext, Window};
use subspace_dashboard::Tokens;
use subspace_dashboard::icon_button::IconButtonSize;

use super::inconsistencies::{controls, strip, surfaces};
use super::{
    Sample, buttons, choices, content, dialogs, feedback, inconsistencies, inputs, lists, menus,
};
use crate::ComponentGallery;
use crate::tests::open_gallery;

pub(crate) type Builders =
    fn(&ComponentGallery, &Tokens, &mut Window, &mut Context<ComponentGallery>) -> Vec<Vec<Sample>>;

/// The state captions of each item, in catalogue order.
pub(crate) fn captions(cx: &mut TestAppContext, builders: Builders) -> Vec<Vec<SharedString>> {
    let (gallery, mut cx) = open_gallery(cx);
    cx.update(|window, cx| {
        let tokens = Tokens::current(cx);
        gallery.update(cx, |gallery, cx| {
            builders(gallery, &tokens, window, cx)
                .into_iter()
                .map(|samples| samples.into_iter().map(|sample| sample.state).collect())
                .collect()
        })
    })
}

pub(crate) fn every_state_is_captioned(items: &[Vec<SharedString>]) -> bool {
    items
        .iter()
        .all(|states| !states.is_empty() && states.iter().all(|state| !state.is_empty()))
}

#[gpui_kit::test]
fn button_builders_caption_every_state(cx: &mut TestAppContext) {
    let items = captions(cx, |g, t, w, cx| {
        vec![
            buttons::shared_icon_button(g, t, w, cx),
            buttons::latched_icon_buttons(g, t, w, cx),
            buttons::action_buttons(g, t, w, cx),
            buttons::action_states(g, t, w, cx),
            buttons::menu_buttons(g, t, w, cx),
            buttons::header_buttons(g, t, w, cx),
            buttons::icon_buttons(g, t, w, cx),
            buttons::toolbar_buttons(g, t, w, cx),
            buttons::retry_action(g, t, w, cx),
            buttons::recovery_action(g, t, w, cx),
            buttons::configure_action(g, t, w, cx),
            buttons::service_link(g, t, w, cx),
        ]
    });
    assert_eq!(items.len(), buttons::ITEMS.len());
    assert!(every_state_is_captioned(&items));
}

#[gpui_kit::test]
fn live_icon_button_follows_all_four_global_sizes(cx: &mut TestAppContext) {
    let (gallery, mut cx) = open_gallery(cx);
    for choice in IconButtonSize::ALL {
        cx.update(|window, cx| {
            choice.apply(cx);
            window.render_frame(cx);
            let target = window.find("gallery-sample:shared-icon-button");
            let glyph = window
                .within("gallery-sample:shared-icon-button")
                .find("toolbar-icon");
            assert_eq!(target.label(), Some("Search"));
            assert_eq!(
                target.bounds().size.width,
                choice.metrics().target().to_pixels(window.rem_size())
            );
            assert_eq!(
                target.bounds().size.height,
                choice.metrics().target().to_pixels(window.rem_size())
            );
            assert_eq!(
                glyph.bounds().size.width,
                choice.metrics().glyph().to_pixels(window.rem_size())
            );
            assert_eq!(
                glyph.bounds().size.height,
                choice.metrics().glyph().to_pixels(window.rem_size())
            );
            assert_eq!(IconButtonSize::current(cx), choice);
            let tokens = Tokens::current(cx);
            gallery.update(cx, |gallery, cx| {
                let samples = buttons::shared_icon_button(gallery, &tokens, window, cx);
                assert_eq!(samples.len(), 1);
                assert_eq!(samples[0].state.as_ref(), choice.label());
            });
        });
    }
}

#[gpui_kit::test]
fn latched_pair_follows_all_four_global_sizes(cx: &mut TestAppContext) {
    let (gallery, mut cx) = open_gallery(cx);
    for choice in IconButtonSize::ALL {
        cx.update(|window, cx| {
            choice.apply(cx);
            window.render_frame(cx);
            for (id, pressed) in [
                ("gallery-sample:latched-off", false),
                ("gallery-sample:latched-on", true),
            ] {
                let target = window.find(id);
                let glyph = window.within(id).find("toolbar-icon");
                assert_eq!(target.role(), Some(Role::Button));
                assert_eq!(target.label(), Some("Wrap"));
                assert_eq!(target.checked(), Some(pressed));
                assert_eq!(
                    target.bounds().size.width,
                    choice.metrics().target().to_pixels(window.rem_size())
                );
                assert_eq!(
                    target.bounds().size.height,
                    choice.metrics().target().to_pixels(window.rem_size())
                );
                assert_eq!(
                    glyph.bounds().size.width,
                    choice.metrics().glyph().to_pixels(window.rem_size())
                );
                assert_eq!(
                    glyph.bounds().size.height,
                    choice.metrics().glyph().to_pixels(window.rem_size())
                );
            }
            let tokens = Tokens::current(cx);
            gallery.update(cx, |gallery, cx| {
                let samples = buttons::latched_icon_buttons(gallery, &tokens, window, cx);
                let captions: Vec<_> = samples.iter().map(|sample| sample.state.as_ref()).collect();
                assert_eq!(
                    captions,
                    [
                        format!("Off, {}", choice.label()),
                        format!("On, {}", choice.label())
                    ]
                );
                assert_eq!(IconButtonSize::current(cx), choice);
            });
        });
    }
}

#[gpui_kit::test]
fn choice_and_input_builders_caption_every_state(cx: &mut TestAppContext) {
    let items = captions(cx, |g, t, w, cx| {
        vec![
            choices::segmented_controls(g, t, w, cx),
            choices::tab_bars(g, t, w, cx),
            choices::dock::samples(g, t, w, cx),
            choices::unread_marks(g, t, w, cx),
            choices::switches(g, t, w, cx),
            choices::toggle_rows(g, t, w, cx),
            choices::sliders(g, t, w, cx),
            inputs::role_inputs(g, t, w, cx),
            inputs::labelled_inputs(g, t, w, cx),
            inputs::code_editor(g, t, w, cx),
            inputs::forms::form_fields(g, t, w, cx),
            inputs::forms::headings(g, t, w, cx),
            inputs::forms::notes(g, t, w, cx),
            inputs::forms::field_errors(g, t, w, cx),
            inputs::forms::path_inputs(g, t, w, cx),
            inputs::forms::textareas(g, t, w, cx),
            inputs::forms::secrets(g, t, w, cx),
            inputs::forms::reveal_buttons(g, t, w, cx),
        ]
    });
    assert_eq!(items.len(), choices::ITEMS.len() + inputs::ITEMS.len());
    assert!(every_state_is_captioned(&items));
}

#[gpui_kit::test]
fn menu_and_dialog_builders_caption_every_state(cx: &mut TestAppContext) {
    let items = captions(cx, |g, t, w, cx| {
        vec![
            menus::dropdown_menu(g, t, w, cx),
            menus::selects(g, t, w, cx),
            menus::color_selects(g, t, w, cx),
            menus::menu_anchors(g, t, w, cx),
            menus::owned::anchors(g, t, w, cx),
            menus::form_popovers(g, t, w, cx),
            menus::owned::row_menus(g, t, w, cx),
            menus::owned::repository_pickers(g, t, w, cx),
            dialogs::frames(g, t, w, cx),
            dialogs::footers(g, t, w, cx),
            dialogs::backdrop(g, t, w, cx),
        ]
    });
    assert_eq!(items.len(), menus::ITEMS.len() + dialogs::ITEMS.len());
    assert!(every_state_is_captioned(&items));
}

#[gpui_kit::test]
fn feedback_builders_caption_every_state(cx: &mut TestAppContext) {
    let items = captions(cx, |g, t, w, cx| {
        vec![
            super::copy_details::samples(g, t, w, cx),
            super::github_sign_in::samples(g, t, w, cx),
            feedback::skeleton(g, t, w, cx),
            feedback::loading_line(g, t, w, cx),
            feedback::empty_line(g, t, w, cx),
            feedback::empty(g, t, w, cx),
            feedback::configuration(g, t, w, cx),
            feedback::error_box(g, t, w, cx),
            feedback::error_box_with_settings(g, t, w, cx),
            feedback::error_box_with_actions(g, t, w, cx),
            feedback::error_banner(g, t, w, cx),
            feedback::error_banner_with_actions(g, t, w, cx),
            feedback::toasts(g, t, w, cx),
        ]
    });
    assert_eq!(items.len(), feedback::ITEMS.len());
    assert!(every_state_is_captioned(&items));
}

#[gpui_kit::test]
fn content_and_list_builders_caption_every_state(cx: &mut TestAppContext) {
    let items = captions(cx, |g, t, w, cx| {
        vec![
            content::disclosure::samples(g, t, w, cx),
            content::badges(g, t, w, cx),
            content::reference_cards(g, t, w, cx),
            content::reference_frames(g, t, w, cx),
            content::pills(g, t, w, cx),
            content::external(g, t, w, cx),
            content::external_marked(g, t, w, cx),
            content::truncated_text(g, t, w, cx),
            content::plain_text(g, t, w, cx),
            content::rich_text(g, t, w, cx),
            content::find_in_page(g, t, w, cx),
            content::media_tile(g, t, w, cx),
            content::file_chip(g, t, w, cx),
            content::attachment_thumbnails(g, t, w, cx),
            content::group_heading(g, t, w, cx),
            content::toolbar_labels(g, t, w, cx),
            content::person(g, t, w, cx),
            content::group(g, t, w, cx),
            lists::list_render(g, t, w, cx),
            lists::native_grid(g, t, w, cx),
            lists::chart(g, t, w, cx),
        ]
    });
    let expected = content::ITEMS.len() + lists::ITEMS.len();
    assert_eq!(items.len(), expected);
    assert!(every_state_is_captioned(&items));
}

#[gpui_kit::test]
fn inconsistency_builders_show_the_closest_shared_component(cx: &mut TestAppContext) {
    let items = captions(cx, |g, t, w, cx| {
        vec![
            controls::mode_switch(g, t, w, cx),
            controls::selected_button_row(g, t, w, cx),
            controls::check_run_link(g, t, w, cx),
            surfaces::inline_confirmation(g, t, w, cx),
        ]
    });
    assert_eq!(items.len() + 1, inconsistencies::ITEMS.len());
    assert!(every_state_is_captioned(&items));
    assert!(
        items
            .iter()
            .flatten()
            .all(|state| state.starts_with("Shared"))
    );
}

#[gpui_kit::test]
fn the_strip_captions_each_replica_with_its_height_and_radius(cx: &mut TestAppContext) {
    let items = captions(cx, |g, t, w, cx| vec![strip::size_and_radius(g, t, w, cx)]);
    let states = items.concat();
    assert!(states.iter().all(|state| state.starts_with("replica: ")));
    assert!(
        states
            .iter()
            .any(|state| state.as_ref() == "replica: badge::badge 16/7 px")
    );
    assert!(
        states
            .iter()
            .any(|state| state.as_ref() == "replica: github::mode_switch 20/4 px")
    );
    assert!(
        states
            .iter()
            .all(|state| !state.starts_with("replica: action::header_button "))
    );
}

mod sections;
