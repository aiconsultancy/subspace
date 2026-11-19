use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{ClipboardItem, TestAppContext};

use super::opened;
use crate::tests::offline::offline;
use crate::tests::{has, press, render};

#[gpui_kit::test]
fn header_menu_copies_the_complete_issue_number_without_leaving_detail(cx: &mut TestAppContext) {
    let fixture = offline();
    let (_board, cx) = opened(&fixture, cx);
    cx.update(|window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("unchanged".into()));
        assert_eq!(window.find("issue-identifier").label(), Some("SUB-1"));
        window.right_click("issue-identifier", cx);
    });
    render(cx);
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("unchanged".into())
    );
    cx.update(|window, cx| {
        assert_eq!(
            window.within("popup-menu").find(0usize).label(),
            Some("Copy issue number")
        );
        window.within("popup-menu").click(0usize, cx);
    });
    render(cx);
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("SUB-1".into())
    );
    cx.update(|window, _| assert!(window.try_find("popup-menu").is_none()));
    assert!(has(cx, "issues-detail"));
    copy_shortcut_works("y", cx);

    cx.update(|window, cx| window.right_click("issue-identifier", cx));
    render(cx);
    assert!(has(cx, "popup-menu"));
    press(cx, "escape");
    render(cx);
    cx.update(|window, _| assert!(window.try_find("popup-menu").is_none()));
    assert!(has(cx, "issues-detail"), "Escape dismisses only the popup");
    copy_shortcut_works("cmd-.", cx);
}

fn copy_shortcut_works(key: &str, cx: &mut gpui_kit::VisualTestContext) {
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("unchanged".into())));
    press(cx, key);
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("SUB-1".into()),
        "native dismissal returns focus to the detail key context"
    );
}
