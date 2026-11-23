//! The navigation column's two trees share its height without hiding an
//! Application row.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyView, App, AppContext as _, Context, IntoElement, Render, TestAppContext, Window, div, px,
    size,
};
use subspace_core::credentials::GitHubAccounts;
use subspace_core::settings::SettingsFile;
use subspace_lucide::IconName;

use super::Settings;
use super::extensions::{SettingsExtension, SettingsExtensions};
use crate::theme;

struct Blank;

impl Render for Blank {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

fn blank(_: &mut Window, cx: &mut App) -> AnyView {
    cx.new(|_| Blank).into()
}

const EXTENSIONS: &[SettingsExtension] = &[
    SettingsExtension::new("first", "First", IconName::Settings, blank),
    SettingsExtension::new("second", "Second", IconName::Globe, blank),
    SettingsExtension::new("third", "Third", IconName::Ship, blank),
];

const BOARDS: [&str; 7] = ["one", "two", "three", "four", "five", "six", "seven"];

/// The Application rows the tree shows: its own pages and the extensions.
fn application_rows() -> usize {
    super::view::navigation::app_rows(false, false) + EXTENSIONS.len()
}

#[gpui_kit::test]
fn every_application_row_shows_above_the_spaces_in_a_standard_window(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        theme::preview_tests::init_app(cx, root.path(), root.path().join("state.json"));
        theme::initialize(cx);
        let boards = BOARDS
            .map(|id| crate::registry::BoardRegistration {
                id,
                ..super::navigation_tests::registration()
            })
            .to_vec();
        cx.set_global(crate::registry::IncludedBoards(boards));
        cx.set_global(SettingsExtensions(EXTENSIONS));
    });
    let file = SettingsFile::open(&root.path().join("config.json")).unwrap();
    let settings =
        cx.add_window(move |window, cx| Settings::new(file, GitHubAccounts::default(), window, cx));
    // The capture size: the dialog's body is 640 px tall.
    cx.simulate_window_resize(settings.into(), size(px(1440.), px(900.)));
    cx.update_window(settings.into(), |_, window, cx| {
        window.render_frame(cx);
        let application = window.find("settings-app-heading").bounds();
        let spaces = window.find("settings-boards-heading").bounds();
        // Each row is 2 rem, and the Spaces section starts 0.75 rem lower.
        let rows = (application_rows() as f32 * 2. + 0.75) * f32::from(window.rem_size());
        assert!(
            f32::from(spaces.top() - application.bottom()) >= rows,
            "{} Application rows need {rows} px; the tree has {}",
            application_rows(),
            f32::from(spaces.top() - application.bottom())
        );
    })
    .unwrap();
}
