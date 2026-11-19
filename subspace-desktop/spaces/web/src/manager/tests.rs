use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    App, AppContext as _, Bounds, Global, TestAppContext, Window, WindowBounds, WindowOptions, px,
    size,
};
use subspace_framework::core::config;
use subspace_framework::core::credentials::{Credentials, GITHUB_ACCOUNT_KEY};
use subspace_framework::dashboard::{Services, prefs};

use super::Manager;

struct Changes(usize);
impl Global for Changes {}

fn applied(_: &mut Window, cx: &mut App) -> Result<(), String> {
    let path = cx.global::<Services>().config_file.clone();
    cx.global_mut::<Services>().config = Arc::new(config::load(&path).0);
    cx.global_mut::<Changes>().0 += 1;
    Ok(())
}

fn open(
    root: &std::path::Path,
    cx: &mut TestAppContext,
) -> (gpui_kit::AnyWindowHandle, gpui_kit::Entity<Manager>) {
    open_with_credentials(root, cx, None)
}

pub(super) fn open_with_credentials(
    root: &std::path::Path,
    cx: &mut TestAppContext,
    credentials: Option<Arc<Credentials>>,
) -> (gpui_kit::AnyWindowHandle, gpui_kit::Entity<Manager>) {
    let path = root.join("config.json");
    std::fs::write(&path, "{\"futureSetting\":42}").unwrap();
    cx.update(gpui_kit::init);
    cx.update(|cx| {
        let config = config::load(&path).0;
        let credentials =
            credentials.unwrap_or_else(|| Arc::new(Credentials::from_system(&config)));
        let mut product = subspace_framework::product::fixture::products()
            .unwrap()
            .remove(0);
        product.release_repository =
            subspace_framework::product::fixture::RELEASE_REPOSITORY.into();
        let context = subspace_framework::product::ProductContext::new(
            product,
            subspace_framework::product::PlatformEnv {
                os: subspace_framework::product::OsKind::Macos,
                home: root.to_path_buf(),
                xdg_config_home: None,
                xdg_state_home: None,
                app_data: None,
                local_app_data: None,
            },
        );
        cx.set_global(subspace_framework::dashboard::CurrentProduct(
            std::sync::Arc::new(context),
        ));
        cx.set_global(Services {
            config: Arc::new(config),
            credentials,
            tools: Default::default(),
            search: None,
            config_file: path,
        });
        cx.set_global(Changes(0));
        prefs::init_file(None, cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(1100.), px(1500.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| Manager::new(window, cx, applied)),
        )
        .unwrap()
    })
}

#[gpui_kit::test]
fn native_import_confirmation_rename_and_removal_preserve_unrelated_config(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let (handle, manager) = open(root.path(), cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("web-add", cx);
        window.click("web-id", cx);
        window.input("test-website", cx);
        window.click("web-name", cx);
        window.input("Test website", cx);
        window.click("web-location", cx);
        window.input("https://example.com/", cx);
        window.click("web-validate", cx);
        assert!(manager.read(cx).busy);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            manager.read(cx).pending.is_some(),
            "busy={}, error={:?}",
            manager.read(cx).busy,
            manager.read(cx).error
        );
        assert!(cx.global::<Services>().config.web_boards.is_empty());
        window.click("web-confirm-install", cx);
        assert_eq!(cx.global::<Changes>().0, 1);
        assert_eq!(
            cx.global::<Services>().config.web_boards[0].id,
            "test-website"
        );
        window.click("web-edit-test-website", cx);
        window.click("web-name", cx);
        window.press("secondary-a", cx);
        window.input("Renamed website", cx);
        window.click("web-rename", cx);
        assert_eq!(
            cx.global::<Services>().config.web_boards[0].title,
            "Renamed website"
        );
        window.click("web-remove-test-website", cx);
        assert_eq!(cx.global::<Changes>().0, 2);
        window.click("web-remove-confirm", cx);
        assert!(cx.global::<Services>().config.web_boards.is_empty());
        assert_eq!(cx.global::<Changes>().0, 3);
    })
    .unwrap();
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("config.json")).unwrap())
            .unwrap();
    assert_eq!(saved["futureSetting"], 42);
}

#[gpui_kit::test]
fn account_switch_cancels_prepared_result_and_weather_uses_normal_source_form(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let (handle, manager) = open(root.path(), cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("web-weather", cx);
        let editor = manager.read(cx).editor.as_ref().unwrap();
        assert_eq!(editor.identity(cx), ("weather".into(), "Weather".into()));
        assert_eq!(
            editor.board_path.read(cx).value(),
            "subspace-desktop/web-spaces/examples/weather"
        );
        assert_eq!(editor.source_kind(cx), super::model::SourceKind::Repository);
        window.click("web-cancel", cx);
        window.click("web-add", cx);
        window.click("web-validate", cx);
        assert!(manager.read(cx).error.is_some());
        assert!(manager.read(cx).pending.is_none());
        window.click("web-id", cx);
        window.input("test-cancelled", cx);
        window.click("web-name", cx);
        window.input("Cancelled import", cx);
        window.click("web-location", cx);
        window.input("https://example.com/", cx);
        window.click("web-validate", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(
            manager.read(cx).pending.is_some(),
            "busy={}, error={:?}",
            manager.read(cx).busy,
            manager.read(cx).error
        );
        prefs::set(cx, GITHUB_ACCOUNT_KEY, "test-only-new-account");
    });
    cx.run_until_parked();
    cx.update(|cx| {
        let manager = manager.read(cx);
        assert!(!manager.busy);
        assert!(manager.offer.is_none());
        assert!(manager.pending.is_none());
        assert_eq!(cx.global::<Changes>().0, 0);
    });
}
