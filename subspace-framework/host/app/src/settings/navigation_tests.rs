use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{App, AppContext as _, TestAppContext, Window};
use subspace_core::credentials::GitHubAccounts;
use subspace_core::settings::SettingsFile;
use subspace_dashboard::prefs;
use subspace_dashboard::settings::{BoardSettings, SettingsSection};
use subspace_lucide::IconName;

use super::{Settings, SettingsPage};
use crate::theme::{self, THEME_PREF};

const TREE_CHILDREN: &[SettingsSection] = &[SettingsSection {
    id: "nested",
    title: "Nested",
    icon: IconName::Settings,
    children: &[],
    features: &[],
}];

fn unused_factory(_: &mut Window, _: &mut App) -> subspace_dashboard::DashboardHandle {
    panic!("settings navigation must not construct a board")
}

fn no_tools(
    _: &subspace_agent_tools::ToolEnvironment,
) -> Vec<std::sync::Arc<dyn subspace_agent_tools::AgentTool>> {
    Vec::new()
}

#[gpui_kit::test]
fn configuration_opens_the_registered_board_page(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        theme::preview_tests::init_app(cx, root.path(), root.path().join("state.json"));
        theme::initialize(cx);
        cx.set_global(crate::registry::IncludedBoards(vec![registration()]));
    });
    let file = SettingsFile::open(&root.path().join("config.json")).unwrap();
    let settings =
        cx.add_window(move |window, cx| Settings::new(file, GitHubAccounts::default(), window, cx));
    settings
        .update(cx, |settings, _, cx| {
            settings.select_initial_board("future", cx);
            assert_eq!(settings.page, SettingsPage::Board("future"));
            assert_eq!(
                settings.navigation.selected_page(cx),
                Some(SettingsPage::Board("future"))
            );
        })
        .unwrap();
}

#[gpui_kit::test]
fn first_run_configuration_is_saved_and_read_without_a_precreated_file(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        theme::preview_tests::init_app(cx, root.path(), root.path().join("state.json"));
        theme::initialize(cx);
    });
    let path = root.path().join("config.json");
    assert!(!path.exists());
    let file = SettingsFile::open(&path).unwrap();
    let settings =
        cx.add_window(move |window, cx| Settings::new(file, GitHubAccounts::default(), window, cx));
    settings
        .update(cx, |settings, window, cx| {
            settings
                .form
                .repos
                .update(cx, |list, cx| list.toggle("owner/project", cx));
            settings.save(window, cx).unwrap();
        })
        .unwrap();
    let (config, notices) = subspace_core::config::load(&path);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(config.repos, ["owner/project"]);
    assert_eq!(
        SettingsFile::open(&path).unwrap().values().repos,
        ["owner/project"]
    );
}

#[gpui_kit::test]
fn native_settings_tree_uses_arrow_selection_and_expansion(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        theme::preview_tests::init_app(cx, root.path(), root.path().join("state.json"));
        prefs::set(cx, THEME_PREF, "Solarized Light");
        theme::initialize(cx);
        cx.set_global(crate::registry::IncludedBoards(vec![registration()]));
    });
    let path = root.path().join("config.json");
    std::fs::write(&path, "{}").unwrap();
    let file = SettingsFile::open(&path).unwrap();
    let settings =
        cx.add_window(move |window, cx| Settings::new(file, GitHubAccounts::default(), window, cx));
    focus_tree(settings, cx);
    let (last, _) = super::view::navigation::last_app_item();
    cx.update_window(settings.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click(last, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(settings.into(), |_, window, cx| {
        window.press("down", cx);
        window.press("right", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_expanded(settings, cx);
    cx.update_window(settings.into(), |_, window, cx| {
        window.render_frame(cx);
        window.press("down", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(matches!(
        settings.read_with(cx, |settings, _| settings.page.clone()),
        Ok(SettingsPage::BoardSection { .. })
    ));
}

pub(super) fn registration() -> crate::registry::BoardRegistration {
    crate::registry::BoardRegistration {
        settings_editor: None,
        id: "future",
        title: "Future".into(),
        description: "Future board".into(),
        icon: None,
        help: "",
        help_images: &[],
        searchable: false,
        create: std::sync::Arc::new(unused_factory),
        tools: no_tools,
        start_background: |_| {},
        stop_background: |_| {},
        retain_when_disabled: false,
        configuration: None,
        artwork: subspace_dashboard::BoardArtwork::default(),
        connection: Default::default(),
        contracts: Default::default(),
        fields: None,
        settings: BoardSettings {
            icon: None,
            sections: TREE_CHILDREN,
            features: &[],
            notifications: false,
            notification_defaults: Default::default(),
            background_updates: false,
            store_by_default: false,
            live_features: &[],
            description: "",
        },
    }
}

fn focus_tree(settings: gpui_kit::WindowHandle<Settings>, cx: &mut TestAppContext) {
    settings
        .update(cx, |settings, window, cx| {
            settings
                .navigation
                .state
                .update(cx, |tree, cx| tree.focus(window, cx));
        })
        .unwrap();
}

fn assert_expanded(settings: gpui_kit::WindowHandle<Settings>, cx: &mut TestAppContext) {
    settings
        .update(cx, |settings, _, cx| {
            assert!(
                settings
                    .navigation
                    .board_state
                    .read(cx)
                    .selected_entry()
                    .unwrap()
                    .is_expanded()
            );
        })
        .unwrap();
}

#[test]
fn every_page_title_is_a_breadcrumb_from_settings() {
    let boards = [registration()];
    let trail = |page: SettingsPage| {
        super::view::navigation::page_trail(&page, &boards, &[])
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" / ")
    };
    assert_eq!(
        trail(SettingsPage::General),
        "Settings / Application / General"
    );
    assert_eq!(
        trail(SettingsPage::Account),
        "Settings / Application / Account"
    );
    assert_eq!(trail(SettingsPage::Chat), "Settings / Application / Chat");
    assert_eq!(trail(SettingsPage::Mcp), "Settings / Application / MCP");
    assert_eq!(
        trail(SettingsPage::Experiments),
        "Settings / Application / Experiments"
    );
    #[cfg(debug_assertions)]
    assert_eq!(
        trail(SettingsPage::Developer),
        "Settings / Application / Developer"
    );
    assert_eq!(
        trail(SettingsPage::Board("future")),
        "Settings / Space / Future"
    );
    assert_eq!(
        trail(SettingsPage::BoardSection {
            board: "future",
            path: vec![0],
        }),
        "Settings / Space / Future / Nested"
    );
}
