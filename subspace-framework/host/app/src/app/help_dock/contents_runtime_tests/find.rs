//! Help uses the shared find route and keeps an explicit filter route.
use super::{act, content, draw, long_guide, open};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Focusable as _, TestAppContext};

#[gpui_kit::test]
fn help_find_and_shortcut_filter_have_distinct_keyboard_routes(cx: &mut TestAppContext) {
    let (window, panel) = open(cx, content("find", long_guide("Find"), &[]));
    act(cx, window, |window, cx| {
        crate::app::keys::bind_keys(cx);
        panel.read(cx).focus.clone().focus(window, cx);
        window.render_frame(cx);
        window.press(subspace_dashboard::secondary!("f"), cx);
    });
    draw(cx, window);
    act(cx, window, |window, cx| {
        assert!(window.find("find-in-page-bar").visible());
        assert!(
            !panel
                .read(cx)
                .query
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
        window.press("escape", cx);
    });
    draw(cx, window);
    act(cx, window, |window, cx| {
        assert!(window.try_find("find-in-page-bar").is_none());
        window.press(subspace_dashboard::secondary!("alt-f"), cx);
    });
    draw(cx, window);
    act(cx, window, |window, cx| {
        assert!(
            panel
                .read(cx)
                .query
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        )
    });
}

#[gpui_kit::test]
fn content_and_filter_escape_close_find_before_help(cx: &mut TestAppContext) {
    use super::super::HelpPanelEvent;
    use std::{cell::Cell, rc::Rc};
    let (window, panel) = open(cx, content("escape", long_guide("Escape"), &[]));
    let closed = Rc::new(Cell::new(0));
    let count = closed.clone();
    let _sub = cx.update(|cx| {
        cx.subscribe(&panel, move |_, event, cx| {
            if matches!(event, HelpPanelEvent::Close) {
                count.set(count.get() + 1);
            }
            let _ = cx;
        })
    });
    act(cx, window, |_, cx| crate::app::keys::bind_keys(cx));
    for filter in [false, true] {
        let before = closed.get();
        act(cx, window, |window, cx| {
            panel.read(cx).focus.clone().focus(window, cx);
            window.press(subspace_dashboard::secondary!("f"), cx);
        });
        draw(cx, window);
        act(cx, window, |window, cx| {
            if filter {
                window.click("help-shortcuts-filter", cx);
            } else {
                panel.read(cx).focus.clone().focus(window, cx);
            }
            window.press("escape", cx);
        });
        draw(cx, window);
        assert_eq!(closed.get(), before, "Find owns the first Escape");
        act(cx, window, |window, cx| {
            assert!(window.try_find("find-in-page-bar").is_none());
            window.press("escape", cx);
        });
        draw(cx, window);
        assert_eq!(
            closed.get(),
            before + 1,
            "closed Find yields the next Escape to Help"
        );
    }
}
