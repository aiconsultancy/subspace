use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};

use super::SettingsPage;
use super::organization_tests::open_settings;

#[gpui_kit::test]
fn the_mcp_page_follows_chat_and_edits_the_draft(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    cx.update_window(settings.into(), |_, window, cx| {
        window.click("settings-app:mcp", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(settings.into(), |_, window, cx| {
        window.click("mcp-enabled", cx);
        window.click("mcp-require-token", cx);
    })
    .unwrap();
    cx.run_until_parked();
    settings
        .update(cx, |settings, _, cx| {
            assert_eq!(settings.page, SettingsPage::Mcp);
            assert!(settings.mcp.enabled);
            assert!(settings.mcp.require_token);
            assert!(settings.has_unsaved_changes(cx));
        })
        .unwrap();
}

#[gpui_kit::test]
fn an_invalid_field_blocks_save_and_a_valid_one_saves_without_restart(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.json");
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            settings
                .mcp
                .port
                .update(cx, |port, cx| port.set_value("70000", window, cx));
            let before = std::fs::read(&path).unwrap();
            let error = settings.save(window, cx).unwrap_err();
            assert!(error.contains("1 to 65535"), "{error}");
            assert_eq!(
                settings.mcp.error.map(|error| error.field),
                Some("mcp.port")
            );
            assert_eq!(std::fs::read(&path).unwrap(), before);

            settings
                .mcp
                .port
                .update(cx, |port, cx| port.set_value("7400", window, cx));
            settings.mcp.enabled = true;
            settings.mcp.set_group("future", false);
            assert!(!settings.save(window, cx).unwrap(), "MCP needs no restart");
            assert!(settings.mcp.error.is_none());
            assert!(!settings.has_unsaved_changes(cx));
        })
        .unwrap();
    let (config, notices) = subspace_core::config::load(&path);
    assert!(notices.is_empty());
    assert!(config.mcp.enabled);
    assert_eq!(config.mcp.port, 7400);
    assert!(!config.mcp.group_enabled("future"));
}
