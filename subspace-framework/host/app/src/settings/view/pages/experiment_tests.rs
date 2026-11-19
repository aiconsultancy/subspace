//! Feature ownership, native draft controls and persistence use the production Settings view.

use std::path::Path;
use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{App, AppContext as _, TestAppContext, Window, WindowHandle, px, size};
use subspace_agent_tools::Catalog;
use subspace_core::config::Config;
use subspace_core::credentials::Credentials;
use subspace_core::credentials::GitHubAccounts;
use subspace_core::features::{FeatureSpec, FeatureStage};
use subspace_core::settings::SettingsFile;
use subspace_dashboard::Services;
use subspace_dashboard::settings::SettingsSection;
use subspace_lucide::IconName;

use crate::settings::Settings;
use crate::settings::form::SettingsPage;
use crate::theme;

const PROTOTYPES: FeatureSpec = FeatureSpec {
    id: "board.board-lab.prototype-views",
    title: "Show prototype views",
    description: "Show retained prototypes. Save and restart to apply.",
    stage: FeatureStage::Experimental,
    default_enabled: false,
    toggleable: true,
};
const NESTED: FeatureSpec = FeatureSpec {
    id: "board.future.nested-preview",
    title: "Future nested preview",
    ..PROTOTYPES
};
const LIVE: FeatureSpec = FeatureSpec {
    id: "board.board-lab.live-display",
    title: "Live display",
    stage: FeatureStage::Stable,
    ..PROTOTYPES
};
const SECTIONS: &[SettingsSection] = &[SettingsSection {
    id: "nested",
    title: "Nested",
    icon: IconName::Settings,
    children: &[],
    features: &[NESTED],
}];
const PROTOTYPES_CONTROL: &str = "feature-board.board-lab.prototype-views";
const NESTED_CONTROL: &str = "feature-board.future.nested-preview";

fn open(root: &Path, cx: &mut TestAppContext) -> WindowHandle<Settings> {
    cx.update(|cx| {
        theme::preview_tests::init_app(cx, root, root.join("state.json"));
        theme::initialize(cx);
        let mut board_lab = crate::settings::navigation_tests::registration();
        board_lab.id = "board-lab";
        board_lab.title = "Board Lab".into();
        board_lab.settings.features = &[PROTOTYPES, LIVE];
        board_lab.settings.live_features = &[LIVE.id];
        board_lab.settings.sections = &[];
        let mut future = crate::settings::navigation_tests::registration();
        future.settings.sections = SECTIONS;
        cx.set_global(crate::registry::IncludedBoards(vec![board_lab, future]));
    });
    std::fs::write(root.join("config.json"), r#"{"boards":{"board-lab":true}}"#).unwrap();
    reopen(root, cx)
}

#[gpui_kit::test]
fn live_board_feature_saves_without_restart(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    install_services(root.path(), cx);
    save_live_on(settings, root.path(), cx);
    save_live_off_and_restart_feature(root.path(), cx);
    cx.update(|cx| assert!(cx.global::<Services>().config.feature_enabled(&LIVE)));
}

fn install_services(root: &Path, cx: &mut TestAppContext) {
    cx.update(|cx| {
        let mut config = Config::default();
        config.features.insert("board.unknown.kept".into(), true);
        cx.set_global(Services {
            credentials: Arc::new(Credentials::from_system(&config)),
            config: Arc::new(config),
            tools: Catalog::new(Vec::new()).unwrap(),
            search: None,
            config_file: root.join("config.json"),
        });
    });
}

fn save_live_on(settings: WindowHandle<Settings>, root: &Path, cx: &mut TestAppContext) {
    page(settings, SettingsPage::Experiments, cx);
    rendered(settings, cx, |window, _| {
        assert!(
            window
                .try_find("feature-board.board-lab.live-display")
                .is_none()
        )
    });
    page(settings, SettingsPage::Board("board-lab"), cx);
    assert_eq!(
        rendered(settings, cx, |window, _| window
            .find("feature-board.board-lab.live-display")
            .checked()),
        Some(false)
    );
    click(settings, "feature-board.board-lab.live-display", cx);
    cx.update(|cx| assert!(!cx.global::<Services>().config.feature_enabled(&LIVE)));
    settings
        .update(cx, |settings, window, cx| {
            assert!(
                !settings.save(window, cx).unwrap(),
                "live display needs no restart"
            );
        })
        .unwrap();
    assert_eq!(
        SettingsFile::open(&root.join("config.json"))
            .unwrap()
            .values()
            .features
            .get(LIVE.id),
        Some(&true)
    );
    cx.update(|cx| {
        assert!(cx.global::<Services>().config.feature_enabled(&LIVE));
        assert_eq!(
            cx.global::<Services>()
                .config
                .features
                .get("board.unknown.kept"),
            Some(&true)
        );
    });
}

fn save_live_off_and_restart_feature(root: &Path, cx: &mut TestAppContext) {
    let reopened = reopen(root, cx);
    page(reopened, SettingsPage::Board("board-lab"), cx);
    click(reopened, "feature-board.board-lab.live-display", cx);
    cx.update(|cx| assert!(cx.global::<Services>().config.feature_enabled(&LIVE)));
    let cancelled = reopen(root, cx);
    page(cancelled, SettingsPage::Board("board-lab"), cx);
    assert_eq!(
        rendered(cancelled, cx, |window, _| window
            .find("feature-board.board-lab.live-display")
            .checked()),
        Some(true)
    );
    assert!(!toggle_and_save(cancelled, cx));
    assert_live_and_pi(cx, false);
    assert_eq!(
        SettingsFile::open(&root.join("config.json"))
            .unwrap()
            .values()
            .features
            .get(LIVE.id),
        Some(&false)
    );
    save_restart_feature(cancelled, root, cx);
}

fn save_restart_feature(cancelled: WindowHandle<Settings>, root: &Path, cx: &mut TestAppContext) {
    cancelled
        .update(cx, |settings, _, _| {
            settings
                .form
                .set_feature_enabled(&subspace_core::features::PI_CHAT, true)
        })
        .unwrap();
    assert!(toggle_and_save(cancelled, cx));
    assert_live_and_pi(cx, true);
    let saved = SettingsFile::open(&root.join("config.json")).unwrap();
    assert_eq!(
        saved
            .values()
            .features
            .get(subspace_core::features::PI_CHAT.id),
        Some(&true)
    );
}

fn toggle_and_save(settings: WindowHandle<Settings>, cx: &mut TestAppContext) -> bool {
    click(settings, "feature-board.board-lab.live-display", cx);
    settings
        .update(cx, |settings, window, cx| {
            settings.save(window, cx).unwrap()
        })
        .unwrap()
}

fn assert_live_and_pi(cx: &mut TestAppContext, live: bool) {
    cx.update(|cx| {
        let config = &cx.global::<Services>().config;
        assert_eq!(config.feature_enabled(&LIVE), live);
        assert!(!config.feature_enabled(&subspace_core::features::PI_CHAT));
    });
}

fn reopen(root: &Path, cx: &mut TestAppContext) -> WindowHandle<Settings> {
    let file = SettingsFile::open(&root.join("config.json")).unwrap();
    let settings =
        cx.add_window(move |window, cx| Settings::new(file, GitHubAccounts::default(), window, cx));
    cx.simulate_window_resize(settings.into(), size(px(1000.), px(1500.)));
    settings
}

fn rendered<T>(
    settings: WindowHandle<Settings>,
    cx: &mut TestAppContext,
    check: impl FnOnce(&mut Window, &mut App) -> T,
) -> T {
    cx.update_window(settings.into(), |_, window, cx| {
        window.render_frame(cx);
        check(window, cx)
    })
    .unwrap()
}

fn click(settings: WindowHandle<Settings>, id: &'static str, cx: &mut TestAppContext) {
    rendered(settings, cx, |window, cx| window.click(id, cx));
    cx.run_until_parked();
}

fn page(settings: WindowHandle<Settings>, page: SettingsPage, cx: &mut TestAppContext) {
    settings
        .update(cx, |settings, _, cx| {
            settings.page = page;
            cx.notify();
        })
        .unwrap();
    cx.run_until_parked();
}

fn prototype_checked(settings: WindowHandle<Settings>, cx: &mut TestAppContext) -> Option<bool> {
    rendered(settings, cx, |window, _| {
        window.find(PROTOTYPES_CONTROL).checked()
    })
}

#[gpui_kit::test]
fn application_experiments_exclude_every_board_owned_feature(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    page(settings, SettingsPage::Experiments, cx);
    rendered(settings, cx, |window, _| {
        assert!(window.find("feature-app.pi-chat").visible());
        assert!(window.try_find(PROTOTYPES_CONTROL).is_none());
        assert!(window.try_find(NESTED_CONTROL).is_none());
    });
    page(settings, SettingsPage::Board("board-lab"), cx);
    assert!(rendered(settings, cx, |window, _| window
        .find(PROTOTYPES_CONTROL)
        .visible()));
    page(settings, SettingsPage::Board("future"), cx);
    assert!(rendered(settings, cx, |window, _| window
        .find(NESTED_CONTROL)
        .visible()));
}

#[gpui_kit::test]
fn board_feature_drafts_save_and_reopen_in_both_directions(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    page(settings, SettingsPage::Board("board-lab"), cx);
    click(settings, PROTOTYPES_CONTROL, cx);
    page(settings, SettingsPage::Experiments, cx);
    page(settings, SettingsPage::Board("board-lab"), cx);
    assert_eq!(prototype_checked(settings, cx), Some(true));
    settings
        .update(cx, |settings, window, cx| {
            assert!(settings.form.feature_enabled(&PROTOTYPES));
            assert!(
                settings.save(window, cx).unwrap(),
                "prototype views still require a restart"
            );
        })
        .unwrap();
    let reopened = reopen(root.path(), cx);
    page(reopened, SettingsPage::Board("board-lab"), cx);
    assert_eq!(prototype_checked(reopened, cx), Some(true));
    click(reopened, PROTOTYPES_CONTROL, cx);
    reopened
        .update(cx, |settings, window, cx| {
            assert!(!settings.form.feature_enabled(&PROTOTYPES));
            assert!(settings.save(window, cx).unwrap());
        })
        .unwrap();
    let final_editor = reopen(root.path(), cx);
    page(final_editor, SettingsPage::Board("board-lab"), cx);
    assert_eq!(prototype_checked(final_editor, cx), Some(false));
}

#[gpui_kit::test]
fn disabled_board_experiments_reject_clicks_and_retain_the_draft(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    page(settings, SettingsPage::Board("board-lab"), cx);
    click(settings, PROTOTYPES_CONTROL, cx);
    click(settings, "settings-board-lab-enabled", cx);
    assert!(rendered(settings, cx, |window, _| window
        .find("settings-board-off")
        .visible()));
    click(settings, PROTOTYPES_CONTROL, cx);
    assert_eq!(prototype_checked(settings, cx), Some(true));
    click(settings, "settings-board-lab-enabled", cx);
    assert_eq!(prototype_checked(settings, cx), Some(true));
    click(settings, PROTOTYPES_CONTROL, cx);
    assert_eq!(prototype_checked(settings, cx), Some(false));
}
