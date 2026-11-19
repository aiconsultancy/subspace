use std::{cell::Cell, path::Path, rc::Rc, sync::Arc};

use gpui_kit::{AppContext as _, TestAppContext, test::TestWindowExt as _};
use serde_json::json;
use subspace_dashboard::Services;

use super::Fixture;

fn open(root: &Path, cx: &mut TestAppContext) -> Fixture {
    let fixture = Fixture::open(root, cx);
    fixture.press("escape", cx);
    cx.update(|cx| {
        let mut board = crate::registry::included_registrations(cx)[0].clone();
        board.id = "fixture";
        board.title = "Fixture".into();
        board.settings_editor = Some(crate::settings::board_editor_tests::factory);
        cx.set_global(crate::registry::IncludedBoards(vec![board]));
        let config = subspace_core::config::Config::default();
        cx.set_global(Services {
            credentials: Arc::new(subspace_core::credentials::Credentials::from_system(
                &config,
            )),
            config: Arc::new(config),
            tools: subspace_agent_tools::Catalog::new(vec![]).unwrap(),
            search: None,
            config_file: root.join("config.json"),
        });
    });
    fixture.press(
        if cfg!(target_os = "linux") {
            "ctrl-shift-,"
        } else {
            "secondary-,"
        },
        cx,
    );
    fixture.settings(cx).update(cx, |settings, cx| {
        settings.select_initial_board("fixture", cx)
    });
    cx.run_until_parked();
    fixture
}

fn type_value(fixture: &Fixture, value: &str, cx: &mut TestAppContext) {
    fixture.click("fixture-editor-input", cx);
    fixture.press("secondary-a", cx);
    cx.update_window(fixture.window, |_, window, cx| window.input(value, cx))
        .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn child_editor_typing_notifies_settings_and_footer_save_publishes_live_values(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let fixture = open(root.path(), cx);
    let notifications = Rc::new(Cell::new(0));
    let observed = notifications.clone();
    let settings = fixture.settings(cx);
    let _subscription =
        cx.update(|cx| cx.observe(&settings, move |_, _| observed.set(observed.get() + 1)));
    type_value(&fixture, r#"["second", "edited first"]"#, cx);
    assert!(
        notifications.get() > 0,
        "child editor must notify its outer Settings owner"
    );
    cx.update_window(fixture.window, |_, window, _| {
        assert_ne!(window.find("settings-save").disabled(), Some(true));
    })
    .unwrap();
    fixture.click("settings-save", cx);
    fixture.assert_closed(cx);
    cx.update(|cx| {
        assert_eq!(
            cx.global::<Services>().config.board_settings["fixture"]["ordered"],
            json!(["second", "edited first"])
        )
    });
    assert_eq!(
        subspace_core::config::load_with_boards(&root.path().join("config.json"), ["fixture"])
            .0
            .board_settings["fixture"]["ordered"],
        json!(["second", "edited first"])
    );
}

#[gpui_kit::test]
fn child_editor_dirty_cancel_prompts_and_discard_keeps_disk_unchanged(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let fixture = open(root.path(), cx);
    let original = std::fs::read(root.path().join("config.json")).unwrap();
    type_value(&fixture, r#"["cancelled"]"#, cx);
    fixture.click("settings-cancel", cx);
    assert_eq!(fixture.dialogs(cx), 2);
    fixture.click("settings-discard", cx);
    fixture.assert_closed(cx);
    assert_eq!(
        std::fs::read(root.path().join("config.json")).unwrap(),
        original
    );
    let board = subspace_core::config::board_file(&root.path().join("config.json"), "fixture");
    assert!(!board.exists());
}
