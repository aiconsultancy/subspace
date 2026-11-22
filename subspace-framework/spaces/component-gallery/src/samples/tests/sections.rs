//! The messaging, navigation and shell builders, called directly.

use gpui_kit::TestAppContext;

use super::{captions, every_state_is_captioned};
use crate::samples::{messaging, navigation, shell};

#[gpui_kit::test]
fn messaging_and_navigation_builders_caption_every_state(cx: &mut TestAppContext) {
    let items = captions(cx, |g, t, w, cx| {
        vec![
            messaging::chat_inputs(g, t, w, cx),
            messaging::completion_lists(g, t, w, cx),
            messaging::composers(g, t, w, cx),
            messaging::bubbles(g, t, w, cx),
            messaging::messages(g, t, w, cx),
            messaging::senders(g, t, w, cx),
            messaging::members(g, t, w, cx),
            messaging::agents(g, t, w, cx),
            messaging::externals(g, t, w, cx),
            messaging::crew_meters(g, t, w, cx),
            navigation::nav_buttons(g, t, w, cx),
            navigation::glass_controls(g, t, w, cx),
            navigation::overflow_toolbars(g, t, w, cx),
            navigation::surface_headers(g, t, w, cx),
            navigation::workspace::navigators(g, t, w, cx),
            navigation::workspace::toolbars(g, t, w, cx),
            navigation::workspace::dock_switches(g, t, w, cx),
            navigation::workspace::panes(g, t, w, cx),
            navigation::workspace::inspectors(g, t, w, cx),
            navigation::token_swatches(g, t, w, cx),
        ]
    });
    assert_eq!(
        items.len(),
        messaging::ITEMS.len() + navigation::ITEMS.len()
    );
    assert!(every_state_is_captioned(&items));
}

/// Every shell card links to its components; the headless app registers no
/// shell actions, so each Open button is present but unavailable here.
#[gpui_kit::test]
fn shell_builders_link_components_and_offer_open_when_there_is_an_action(cx: &mut TestAppContext) {
    let items = captions(cx, |g, t, w, cx| {
        vec![
            shell::window(g, t, w, cx),
            shell::toolbar(g, t, w, cx),
            shell::navigation_rail(g, t, w, cx),
            shell::status_bar(g, t, w, cx),
            shell::command_palette(g, t, w, cx),
            shell::help(g, t, w, cx),
            shell::settings_dialog(g, t, w, cx),
            shell::about_dialog(g, t, w, cx),
            shell::report_issue(g, t, w, cx),
            shell::theme_designer(g, t, w, cx),
            shell::toasts(g, t, w, cx),
            shell::chat_panel(g, t, w, cx),
            shell::chat_canvas(g, t, w, cx),
            shell::search_view(g, t, w, cx),
            shell::log_view(g, t, w, cx),
        ]
    });
    assert_eq!(items.len(), shell::ITEMS.len());
    assert!(every_state_is_captioned(&items));
    for (states, surface) in items.iter().zip(shell::SURFACES.iter()) {
        assert_eq!(
            states.len(),
            1 + usize::from(surface.action.is_some()),
            "{}",
            surface.name
        );
    }
}
