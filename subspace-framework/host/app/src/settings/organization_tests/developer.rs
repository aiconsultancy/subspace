//! The debug-only Developer page and its Developer mode switch.

use gpui_kit::TestAppContext;
use subspace_dashboard::prefs;

use super::{SettingsPage, click, open_settings};
use crate::settings::{DEVELOPER_MODE_KEY, developer_mode};

#[gpui_kit::test]
fn developer_mode_starts_on_and_turning_it_off_waits_for_save(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    click(settings, "settings-app:developer", cx);
    settings
        .update(cx, |settings, _, cx| {
            assert_eq!(settings.page, SettingsPage::Developer);
            assert!(settings.developer_mode);
            assert!(!settings.has_unsaved_changes(cx));
        })
        .unwrap();
    click(settings, "settings-developer-mode", cx);
    settings
        .update(cx, |settings, window, cx| {
            assert!(settings.has_unsaved_changes(cx));
            assert_eq!(prefs::get(cx, DEVELOPER_MODE_KEY), None);
            assert!(developer_mode(cx), "the switch is a draft until Save");
            settings.save(window, cx).unwrap();
            assert_eq!(prefs::get(cx, DEVELOPER_MODE_KEY).as_deref(), Some("0"));
            assert!(!developer_mode(cx));
            assert!(!settings.has_unsaved_changes(cx));
        })
        .unwrap();
}

#[gpui_kit::test]
fn developer_settings_open_on_the_developer_page(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, _, cx| {
            settings.select_developer_page(cx);
            assert_eq!(settings.page, SettingsPage::Developer);
        })
        .unwrap();
}
