use std::path::Path;

use gpui_kit::component::button::Button;
use gpui_kit::{
    AnyView, App, AppContext as _, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, TestAppContext, TestSupportExt as _, Window, WindowHandle, div, px,
    size,
};
use subspace_core::credentials::{GITHUB_ACCOUNT_KEY, GitHubAccounts};
use subspace_core::settings::SettingsFile;
use subspace_dashboard::prefs;
use subspace_lucide::IconName;

use crate::settings::extensions::{SettingsExtension, SettingsExtensions};
use crate::settings::{Settings, SettingsPage};
use crate::theme;

pub(super) const INITIAL: &str = "test-only-first-account";
pub(super) const OTHER: &str = "test-only-second-account";
pub(super) const SAVED: &str = "test-only-nonworking-saved-token";
pub(super) const REPLACEMENT: &str = "test-only-nonworking-replacement-token";

const EXTENSIONS: &[SettingsExtension] = &[
    SettingsExtension::new("fixture-import", "Fixture import", IconName::Globe, create)
        .with_github_credentials(),
    SettingsExtension::new("fixture-plain", "Fixture plain", IconName::Settings, create),
];

struct Extension;

impl Render for Extension {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("fixture-extension-view")
            .test_support()
            .child("Import fixture")
            // Exercise the Settings-owned boundary through pointer events.
            // The shell's leave-confirmation lifecycle has separate tests.
            .child(save_button())
            .child(cancel_button())
    }
}

fn save_button() -> Button {
    Button::new("fixture-save")
        .icon(IconName::Save)
        .label("Save")
        .on_click(|_, window, cx| {
            let settings = window.root::<Settings>().flatten().unwrap();
            settings.update(cx, |settings, cx| {
                assert!(settings.save(window, cx).unwrap());
            });
        })
}

fn cancel_button() -> Button {
    Button::new("fixture-cancel")
        .icon(IconName::X)
        .label("Cancel")
        .on_click(|_, window, cx| {
            let settings = window.root::<Settings>().flatten().unwrap();
            settings.update(cx, |settings, cx| {
                settings.restore_preview(window, cx).unwrap();
            });
            window.remove_window();
        })
}

fn create(_: &mut Window, cx: &mut App) -> AnyView {
    cx.new(|_| Extension).into()
}

fn initialize(root: &Path, cx: &mut App) {
    theme::preview_tests::init_app(cx, root, root.join("state.json"));
    theme::initialize(cx);
    prefs::set(cx, GITHUB_ACCOUNT_KEY, INITIAL);
    prefs::save(cx).unwrap();
    let boards = ["github", "actions", "github-copilot-agents"].map(|id| {
        let mut board = crate::settings::navigation_tests::registration();
        board.id = id;
        board.title = id.into();
        board.settings.sections = &[];
        board.settings.features = &[];
        board.connection = subspace_dashboard::settings::ConnectionSettings::new(&[
            subspace_dashboard::settings::ConnectionField::GitHubCredentials,
            subspace_dashboard::settings::ConnectionField::Repositories,
        ]);
        board
    });
    cx.set_global(crate::registry::IncludedBoards(boards.into()));
    cx.set_global(SettingsExtensions(EXTENSIONS));
}

pub(super) fn open(root: &Path, cx: &mut TestAppContext) -> WindowHandle<Settings> {
    cx.update(|cx| initialize(root, cx));
    let path = root.join("config.json");
    std::fs::write(
        &path,
        serde_json::json!({"github": {"token": SAVED}}).to_string(),
    )
    .unwrap();
    let file = SettingsFile::open(&path).unwrap();
    let accounts = GitHubAccounts {
        logins: vec![INITIAL.into(), OTHER.into()],
        active: Some(INITIAL.into()),
        signed_in: Vec::new(),
    };
    let settings = cx.add_window(move |window, cx| {
        let mut settings = Settings::new(file, accounts, window, cx);
        settings.page = SettingsPage::Board("github");
        settings
    });
    cx.simulate_window_resize(settings.into(), size(px(1100.), px(1800.)));
    settings
}
