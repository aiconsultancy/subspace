use super::{Harness, settle};
use crate::overflow_toolbar::{OverflowToolbar, OverflowUnit, PopupEntry};
use gpui_kit::component::{button::Button, menu::PopupMenuItem};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, Styled as _, TestAppContext, VisualTestContext, px};
use std::{cell::RefCell, rc::Rc};

fn actions(calls: Rc<RefCell<Vec<usize>>>, revision: usize) -> Vec<OverflowUnit> {
    (0..3)
        .map(|index| {
            let inline_calls = calls.clone();
            let menu_calls = calls.clone();
            OverflowUnit::inline(
                format!("unit-{index}"),
                30.,
                move |_, _| {
                    let calls = inline_calls.clone();
                    Button::new(format!("inline-{index}"))
                        .w(px(30.))
                        .label("X")
                        .on_click(move |_, _, _| calls.borrow_mut().push(index))
                },
                vec![PopupEntry::new(move || {
                    let calls = menu_calls.clone();
                    PopupMenuItem::new(format!("Action {index} revision {revision}"))
                        .disabled(index == 1 && revision == 0)
                        .checked(index == 2)
                        .on_click(move |_, _, _| calls.borrow_mut().push(index + revision * 10))
                })],
            )
        })
        .collect()
}

struct Fixture {
    calls: Rc<RefCell<Vec<usize>>>,
    toolbar: Entity<OverflowToolbar>,
}

fn mount(cx: &mut TestAppContext) -> (Fixture, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let toolbar = cx.new(|_| OverflowToolbar::new(actions(calls.clone(), 0)));
    let fixture = Fixture {
        calls,
        toolbar: toolbar.clone(),
    };
    let (_, cx) = cx.add_window_view(|_, _| Harness {
        toolbar,
        width: 70.,
        left: 0.,
    });
    settle(cx);
    (fixture, cx)
}

#[gpui_kit::test]
fn native_popup_rejects_disabled_click_and_activates_once(cx: &mut TestAppContext) {
    let (Fixture { calls, .. }, cx) = mount(cx);
    cx.update(|window, cx| {
        window.click("overflow-toolbar-button", cx);
        assert_eq!(window.find(0usize).label(), Some("Action 1 revision 0"));
        assert_eq!(window.find(1usize).label(), Some("Action 2 revision 0"));
        assert!(window.try_find(2usize).is_none());
        window.click(0usize, cx);
        assert!(
            calls.borrow().is_empty(),
            "disabled entry must not activate"
        );
        window.click(1usize, cx);
        assert_eq!(*calls.borrow(), vec![2]);
    });
    settle(cx);
    cx.update(|window, _| {
        assert!(
            window.try_find(0usize).is_none(),
            "activation dismisses popup"
        );
    });
}

#[gpui_kit::test]
fn keyboard_open_confirm_escape_and_focus_return(cx: &mut TestAppContext) {
    let (Fixture { calls, .. }, cx) = mount(cx);
    cx.update(|window, cx| {
        window.focus_next(cx);
        window.focus_next(cx);
        window.render_frame(cx);
        assert_eq!(window.find("overflow-toolbar-button").focused(), Some(true));
        window.press("enter", cx);
        assert!(window.try_find(0usize).is_some());
        window.press("down", cx);
        window.press("down", cx);
        window.press("enter", cx);
        assert_eq!(*calls.borrow(), vec![2]);
    });
    settle(cx);
    cx.update(|window, cx| {
        assert_eq!(window.find("overflow-toolbar-button").focused(), Some(true));
        window.press("enter", cx);
        window.press("escape", cx);
    });
    settle(cx);
    cx.update(|window, _| {
        assert!(window.try_find(0usize).is_none());
        assert_eq!(window.find("overflow-toolbar-button").focused(), Some(true));
        assert_eq!(*calls.borrow(), vec![2]);
    });
}

#[gpui_kit::test]
fn replacing_same_keys_updates_open_menu_and_callbacks(cx: &mut TestAppContext) {
    let (Fixture { calls, toolbar }, cx) = mount(cx);
    cx.update(|window, cx| window.click("overflow-toolbar-button", cx));
    toolbar.update(cx, |toolbar, cx| {
        toolbar.set_units(actions(calls.clone(), 1), cx)
    });
    settle(cx);
    cx.update(|window, cx| {
        assert_eq!(window.find(0usize).label(), Some("Action 1 revision 1"));
        window.click(0usize, cx);
        assert_eq!(*calls.borrow(), vec![11]);
    });
}

#[gpui_kit::test]
fn keyboard_selection_skips_a_disabled_first_entry(cx: &mut TestAppContext) {
    let (Fixture { calls, .. }, cx) = mount(cx);
    cx.update(|window, cx| {
        window.click("overflow-toolbar-button", cx);
        window.press("down", cx);
        window.press("enter", cx);
        assert_eq!(
            *calls.borrow(),
            vec![2],
            "the keyboard selects the first enabled entry, never the disabled one"
        );
    });
}
