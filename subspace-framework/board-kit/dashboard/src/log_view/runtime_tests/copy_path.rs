use super::toolbar::{item, settle};
use crate::log_view::LogView;
use chrono::{TimeZone as _, Utc};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, ClipboardItem, Entity, Role, TestAppContext, VisualTestContext, px, size,
};
use std::path::Path;
use subspace_core::log::{Level, Record};

const FIXTURE_PATH: &str = "/synthetic-test-only/custom-logs/fixture-product.log";
const UNCHANGED: &str = "test-only clipboard sentinel";

fn mount(cx: &mut TestAppContext, path: Option<&Path>) -> (Entity<LogView>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let (handle, view) = cx.update(|cx| {
        gpui_kit::open_window(Default::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut view = LogView::new(window, cx);
                // Keep tests independent of the process-global file writer.
                view.log_path = path.map(Path::to_path_buf);
                view.records = vec![Record {
                    seq: 1,
                    at: Utc.with_ymd_and_hms(2026, 11, 9, 12, 0, 0).unwrap(),
                    level: Level::Info,
                    target: "fixture".into(),
                    message: "Synthetic log record".into(),
                    fields: Vec::new(),
                }];
                view
            })
        })
        .unwrap()
    });
    let mut cx = VisualTestContext::from_window(handle, cx);
    cx.simulate_resize(size(px(2400.), px(600.)));
    settle(&mut cx);
    (view, cx)
}

fn clipboard(cx: &mut gpui_kit::App) -> Option<String> {
    cx.read_from_clipboard().and_then(|item| item.text())
}

fn sentinel(cx: &mut gpui_kit::App) {
    cx.write_to_clipboard(ClipboardItem::new_string(UNCHANGED.into()));
}

#[gpui_kit::test]
fn copy_path_toolbar_uses_pointer_space_and_enter_without_replacing_copy_text(
    cx: &mut TestAppContext,
) {
    let (view, mut cx) = mount(cx, Some(Path::new(FIXTURE_PATH)));
    cx.update(|window, cx| {
        let button = window.find("log-copy-path");
        assert_eq!(button.role(), Some(Role::Button));
        assert_eq!(button.label(), Some("Copy log path"));
        assert!(button.bounds().right() <= window.find("log-folder").bounds().left());
        window.click("log-copy-path", cx);
        assert_eq!(clipboard(cx).as_deref(), Some(FIXTURE_PATH));
        view.read(cx).focus.clone().focus(window, cx);
        for _ in 0..32 {
            if window.find("log-copy-path").focused() == Some(true) {
                break;
            }
            window.press("tab", cx);
        }
        assert_eq!(window.find("log-copy-path").focused(), Some(true));
        for key in ["space", "enter"] {
            sentinel(cx);
            window.press(key, cx);
            assert_eq!(clipboard(cx).as_deref(), Some(FIXTURE_PATH));
        }
        window.click("log-copy", cx);
        assert_eq!(
            clipboard(cx).as_deref(),
            Some(view.read(cx).text().as_str())
        );
        assert_eq!(window.find("log-folder").label(), Some("Open log folder"));
    });
}

#[gpui_kit::test]
fn copy_path_overflow_uses_the_same_command_and_native_keyboard(cx: &mut TestAppContext) {
    let (_, mut cx) = mount(cx, Some(Path::new(FIXTURE_PATH)));
    cx.simulate_resize(size(px(320.), px(600.)));
    settle(&mut cx);
    cx.update(|window, cx| {
        assert!(
            window
                .try_find("log-copy-path")
                .is_none_or(|button| !button.visible())
        );
        window.click("overflow-toolbar-button", cx);
        let path = item(window, "Copy log path");
        assert!(window.within("popup-menu").find(path).visible());
        assert!(item(window, "Open log folder") > path);
        window.within("popup-menu").click(path, cx);
    });
    settle(&mut cx);
    cx.update(|window, cx| {
        assert_eq!(clipboard(cx).as_deref(), Some(FIXTURE_PATH));
        assert!(window.try_find("popup-menu").is_none());
        assert_eq!(window.find("overflow-toolbar-button").focused(), Some(true));
        sentinel(cx);
        window.press("enter", cx);
    });
    settle(&mut cx);
    cx.update(|window, cx| {
        let path = item(window, "Copy log path");
        // A newly opened native menu starts without a selected item.
        for _ in 0..=path {
            window.press("down", cx);
        }
        window.press("enter", cx);
    });
    settle(&mut cx);
    cx.update(|window, cx| {
        assert_eq!(clipboard(cx).as_deref(), Some(FIXTURE_PATH));
        assert!(window.try_find("popup-menu").is_none());
    });
}

#[gpui_kit::test]
fn no_writer_disables_copy_path_in_toolbar_and_overflow(cx: &mut TestAppContext) {
    let (view, mut cx) = mount(cx, None);
    cx.update(|window, cx| {
        sentinel(cx);
        // Pinned Kit disables interaction and focus, but does not publish an
        // AccessKit disabled property. Prove the native behavior instead.
        view.read(cx).focus.clone().focus(window, cx);
        for _ in 0..32 {
            window.press("tab", cx);
            assert_ne!(window.find("log-copy-path").focused(), Some(true));
        }
        view.read(cx).focus.clone().focus(window, cx);
        window.click("log-copy-path", cx);
        window.press("enter", cx);
        window.press("space", cx);
        assert_eq!(clipboard(cx).as_deref(), Some(UNCHANGED));
    });
    cx.simulate_resize(size(px(320.), px(600.)));
    settle(&mut cx);
    cx.update(|window, cx| {
        window.click("overflow-toolbar-button", cx);
        let path = item(window, "Copy log path");
        assert!(window.within("popup-menu").find(path).visible());
        window.within("popup-menu").click(path, cx);
        assert_eq!(clipboard(cx).as_deref(), Some(UNCHANGED));
        window.press("escape", cx);
    });
    settle(&mut cx);
    cx.update(|window, cx| {
        assert!(window.try_find("popup-menu").is_none());
        assert_eq!(clipboard(cx).as_deref(), Some(UNCHANGED));
    });
}
