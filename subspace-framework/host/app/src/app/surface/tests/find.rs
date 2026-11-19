//! Find commands are offered only for a visible Space, never a replacement surface.
use super::{open, update};
use crate::app::{CloseLogView, CloseSearchView, OpenSearchView};
use gpui_kit::TestAppContext;
use subspace_dashboard::log_view::OpenLogView;
#[gpui_kit::test]
fn replacement_surfaces_do_not_offer_hidden_space_find(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let window = open(root.path(), cx);
    let has_find = |window, cx: &mut TestAppContext| {
        let mut present = false;
        update(window, cx, |app, _, cx| {
            present = app
                .shell_commands(cx)
                .iter()
                .any(|c| c.label == "Find in this Space");
        });
        present
    };
    assert!(has_find(window, cx));
    update(window, cx, |app, window, cx| {
        app.open_log_view(&OpenLogView, window, cx)
    });
    assert!(!has_find(window, cx));
    update(window, cx, |app, window, cx| {
        app.open_full_text(&OpenSearchView, window, cx)
    });
    assert!(!has_find(window, cx));
    update(window, cx, |app, window, cx| {
        app.close_full_text(&CloseSearchView, window, cx)
    });
    update(window, cx, |app, window, cx| {
        app.close_log_view(&CloseLogView, window, cx)
    });
    assert!(has_find(window, cx));
}
