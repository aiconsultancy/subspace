use crate::account::{self as native, Status};
use crate::settings::organization_tests::open_settings;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, px, size};

#[gpui_kit::test]
fn settings_only_offer_an_account_when_the_app_registers_a_provider(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let unconfigured = open_settings(root.path(), cx);
    cx.update_window(unconfigured.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("settings-app:account").is_none());
    })
    .unwrap();

    cx.update(|cx| native::fixture("signed-out", cx).unwrap());
    let configured = open_settings(root.path(), cx);
    cx.update_window(configured.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("settings-app:account").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn account_recovery_controls_follow_the_failure_and_do_not_edit_settings(cx: &mut TestAppContext) {
    cx.update(|cx| native::fixture("signed-out", cx).unwrap());
    let root = tempfile::tempdir().unwrap();
    let settings = open_account(root.path(), cx);
    for (state, label, retry) in [
        ("signed-out", "Sign in with Example Identity", false),
        ("signed-in", "Switch account", false),
        ("expired", "Sign in again", false),
        ("offline", "Switch account", true),
        ("denied", "Switch account", false),
    ] {
        cx.update(|cx| native::fixture(state, cx).unwrap());
        settings
            .update(cx, |_, window, _| window.refresh())
            .unwrap();
        cx.update_window(settings.into(), |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("settings-account-sign-in").label(), Some(label));
            assert_eq!(window.try_find("settings-account-retry").is_some(), retry);
        })
        .unwrap();
        settings
            .update(cx, |settings, _, cx| {
                assert!(!settings.has_unsaved_changes(cx))
            })
            .unwrap();
    }
}

#[gpui_kit::test]
fn cancelling_the_browser_returns_to_the_existing_account_controls(cx: &mut TestAppContext) {
    cx.update(|cx| native::fixture("waiting", cx).unwrap());
    let root = tempfile::tempdir().unwrap();
    let settings = open_account(root.path(), cx);
    cx.simulate_window_resize(settings.into(), size(px(760.), px(900.)));
    cx.update_window(settings.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("settings-account-cancel").visible());
        assert!(window.find("settings-account-reopen").visible());
        assert!(window.try_find("settings-account-sign-in").is_none());
    })
    .unwrap();
    cx.update_window(settings.into(), |_, window, cx| {
        window.click("settings-account-cancel", cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| assert_eq!(native::shared(cx).unwrap().read(cx).status, Status::Idle));
    settings
        .update(cx, |settings, _, cx| {
            assert!(!settings.has_unsaved_changes(cx))
        })
        .unwrap();
}

fn open_account(
    root: &std::path::Path,
    cx: &mut TestAppContext,
) -> gpui_kit::WindowHandle<crate::settings::Settings> {
    let settings = open_settings(root, cx);
    cx.update_window(settings.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("settings-account-sign-in").is_none());
    })
    .unwrap();
    settings
        .update(cx, |settings, window, cx| {
            settings
                .navigation
                .state
                .update(cx, |tree, cx| tree.focus(window, cx));
        })
        .unwrap();
    cx.update_window(settings.into(), |_, window, cx| window.press("down", cx))
        .unwrap();
    cx.run_until_parked();
    assert!(matches!(
        settings.read_with(cx, |settings, _| settings.page.clone()),
        Ok(crate::settings::SettingsPage::Account)
    ));
    settings
}

#[gpui_kit::test]
fn device_sign_in_shows_the_code_to_confirm_in_the_browser(cx: &mut TestAppContext) {
    cx.update(|cx| native::fixture("device", cx).unwrap());
    let root = tempfile::tempdir().unwrap();
    let settings = open_account(root.path(), cx);
    cx.simulate_window_resize(settings.into(), size(px(760.), px(900.)));
    cx.update_window(settings.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("settings-account-cancel").visible());
        assert!(window.find("settings-account-reopen").visible());
    })
    .unwrap();
    cx.update(|cx| {
        let account = native::shared(cx).unwrap();
        assert_eq!(account.read(cx).code.as_deref(), Some("ABCD-EFGH"));
    });
    assert_eq!(
        super::status_text(Status::Waiting, false, "WorkOS", Some("ABCD-EFGH")).as_deref(),
        Some("Check that your browser shows the code ABCD-EFGH, then confirm it there.")
    );
    assert_eq!(
        super::status_text(Status::Waiting, false, "WorkOS", None).as_deref(),
        Some("Complete sign-in in your browser.")
    );
}
