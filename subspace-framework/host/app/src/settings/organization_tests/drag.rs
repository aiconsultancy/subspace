use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, WindowHandle};

use super::{Settings, SettingsPage, click, open_settings};

fn assert_order(settings: WindowHandle<Settings>, order: &[&str], cx: &mut TestAppContext) {
    settings
        .read_with(cx, |settings, _| {
            assert_eq!(
                settings
                    .boards
                    .iter()
                    .map(|board| board.id)
                    .collect::<Vec<_>>(),
                order
            );
        })
        .unwrap();
}

fn drag(settings: WindowHandle<Settings>, source: &str, target: &str, cx: &mut TestAppContext) {
    cx.update_window(settings.into(), |_, window, cx| {
        window.render_frame(cx);
        window.drag_to(
            format!("board-order-row-{source}"),
            format!("board-order-row-{target}"),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn native_board_drag_uses_ids_in_both_directions_and_keeps_same_label_routes(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    click(settings, "settings-app:layout", cx);
    drag(settings, "other", "future", cx);
    assert_order(settings, &["other", "future"], cx);
    drag(settings, "other", "other", cx);
    assert_order(settings, &["other", "future"], cx);
    drag(settings, "other", "future", cx);
    assert_order(settings, &["future", "other"], cx);
    settings
        .read_with(cx, |settings, cx| {
            assert_eq!(settings.page, SettingsPage::Layout);
            assert!(!settings.has_unsaved_changes(cx));
        })
        .unwrap();
    drag(settings, "future", "other", cx);
    click(settings, "settings-board:future", cx);
    click(settings, "settings-board:future/nested", cx);
    settings
        .read_with(cx, |settings, cx| {
            assert_eq!(
                settings.page,
                SettingsPage::BoardSection {
                    board: "future",
                    path: vec![0]
                }
            );
            assert!(settings.has_unsaved_changes(cx));
        })
        .unwrap();
}

#[gpui_kit::test]
fn drag_order_alone_saves_without_restart_and_reopens_in_saved_order(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    click(settings, "settings-app:layout", cx);
    drag(settings, "other", "future", cx);
    settings
        .update(cx, |settings, window, cx| {
            assert!(settings.has_unsaved_changes(cx));
            assert!(!settings.save(window, cx).unwrap());
            assert!(!settings.has_unsaved_changes(cx));
        })
        .unwrap();
    let config = subspace_core::config::load(&root.path().join("config.json")).0;
    assert_eq!(config.board_order, ["other", "future"]);
    let reopened = open_settings(root.path(), cx);
    assert_order(reopened, &["other", "future"], cx);
    reopened
        .read_with(cx, |settings, cx| {
            assert!(!settings.has_unsaved_changes(cx))
        })
        .unwrap();
}
