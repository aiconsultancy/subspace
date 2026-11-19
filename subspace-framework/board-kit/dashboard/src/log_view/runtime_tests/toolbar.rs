use crate::{
    icon_button::IconButtonSize,
    log_view::{LogView, LogViewEvent},
};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Role, TestAppContext, VisualTestContext, px, size};
use std::{cell::Cell, rc::Rc};

pub(super) fn settle(cx: &mut VisualTestContext) {
    for _ in 0..6 {
        cx.update(|window, cx| window.render_frame(cx));
        cx.run_until_parked();
    }
}

fn position(window: &mut gpui_kit::Window, label: &str) -> Option<usize> {
    let menu = window.within("popup-menu");
    (0usize..32).find(|index| {
        menu.try_find(*index)
            .is_some_and(|item| item.label() == Some(label))
    })
}

pub(super) fn item(window: &mut gpui_kit::Window, label: &str) -> usize {
    position(window, label).unwrap_or_else(|| panic!("missing popup item {label}"))
}

#[gpui_kit::test]
fn follow_button_reports_pressed_state_and_keeps_the_existing_poller_lifecycle(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let (handle, view) = cx.update(|cx| {
        gpui_kit::open_window(Default::default(), cx, |window, cx| {
            cx.new(|cx| LogView::new(window, cx))
        })
        .unwrap()
    });
    let cx = &mut VisualTestContext::from_window(handle, cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        let follow = window.find("log-follow");
        assert_eq!(follow.role(), Some(Role::Button));
        assert_eq!(follow.label(), Some("Follow"));
        assert_eq!(follow.checked(), Some(true));
        window.click("log-follow", cx);
        assert_eq!(window.find("log-follow").role(), Some(Role::Button));
        assert_eq!(window.find("log-follow").checked(), Some(false));
        view.read(cx).focus.clone().focus(window, cx);
        for _ in 0..16 {
            if window.find("log-follow").focused() == Some(true) {
                break;
            }
            window.press("tab", cx);
        }
        assert_eq!(window.find("log-follow").focused(), Some(true));
        assert!(!view.read(cx).follow && view.read(cx).poll.is_none());
        window.press("space", cx);
        assert_eq!(window.find("log-follow").checked(), Some(true));
        assert!(view.read(cx).follow && view.read(cx).poll.is_some());
        window.press("enter", cx);
        assert_eq!(window.find("log-follow").checked(), Some(false));
        assert!(!view.read(cx).follow && view.read(cx).poll.is_none());
    });
}

#[gpui_kit::test]
fn xl_log_toolbar_keeps_search_and_reopens_hidden_pickers_at_its_overflow_anchor(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        IconButtonSize::ExtraLarge.apply(cx);
    });
    let (view, cx) = cx.add_window_view(LogView::new);
    let closed = Rc::new(Cell::new(false));
    let observed = closed.clone();
    let _subscription = cx.update(|window, cx| {
        window.subscribe(&view, cx, move |_, _: &LogViewEvent, _, _| {
            observed.set(true)
        })
    });
    cx.simulate_resize(size(px(320.), px(600.)));
    settle(cx);
    cx.update(|window, cx| {
        let toolbar = window.find("log-toolbar").bounds();
        let search = window.find("log-search").bounds();
        let trigger = window.find("overflow-toolbar-button").bounds();
        assert!(toolbar.size.height <= px(85.));
        assert!(search.size.width > px(0.) && search.right() < trigger.left());
        assert_eq!(trigger.size.height, px(72.));
        assert!(trigger.right() <= toolbar.right());
        assert!((search.center().y - trigger.center().y).abs() <= px(1.));
        window.click("overflow-toolbar-button", cx);
        let target = item(window, "Target…");
        window.within("popup-menu").click(target, cx);
    });
    settle(cx);
    cx.update(|window, cx| {
        assert!(window.try_find("menu").is_some());
        window.press("escape", cx);
    });
    settle(cx);
    cx.update(|window, cx| {
        assert_eq!(window.find("overflow-toolbar-button").focused(), Some(true));
        window.click("overflow-toolbar-button", cx);
        let follow = item(window, "Follow");
        window.within("popup-menu").click(follow, cx);
    });
    settle(cx);
    cx.update(|window, cx| {
        assert!(!view.read(cx).follow && view.read(cx).poll.is_none());
        window.click("overflow-toolbar-button", cx);
        assert_eq!(
            position(window, "Close"),
            None,
            "Close never moves into overflow"
        );
        window.press("escape", cx);
    });
    settle(cx);
    assert!(!closed.get(), "Escape dismisses the overflow menu first");
    click_header_close(cx);
    assert!(closed.get());
}

fn click_header_close(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        let header = window.find("surface-header").bounds();
        let close = window.find("surface-close");
        assert_eq!(close.label(), Some("Close logs"));
        assert!(close.bounds().right() <= header.right());
        assert!(close.bounds().size.width > px(0.));
        window.click("surface-close", cx);
    });
    cx.run_until_parked();
}
