use super::{open, settle, setup};
use gpui_kit::base::GlobalState;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    DismissEvent, Focusable as _, KeyDownEvent, KeyUpEvent, Keystroke, MouseButton, TestAppContext,
    point, px,
};
use std::{cell::Cell, rc::Rc};

#[gpui_kit::test]
fn anchored_dismiss_subscribers_see_a_closed_and_released_popup(cx: &mut TestAppContext) {
    let (root, cx) = setup(cx);
    let anchor = cx.update(|_, cx| root.read(cx).anchor.clone());
    let observed = Rc::new(Cell::new(None));
    let record = observed.clone();
    let _subscription = cx.update(|window, cx| {
        window.subscribe(&anchor, cx, move |anchor, _: &DismissEvent, _, cx| {
            record.set(Some((
                anchor.read(cx).is_open(),
                GlobalState::is_in_deferred_context(cx),
            )));
        })
    });
    open(cx);
    cx.update(|_, cx| assert!(GlobalState::is_in_deferred_context(cx)));
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert_eq!(observed.get(), Some((false, false)));
}

#[gpui_kit::test]
fn anchored_reopen_from_a_dismiss_subscriber_keeps_the_new_popup_registered(
    cx: &mut TestAppContext,
) {
    let (root, cx) = setup(cx);
    let anchor = cx.update(|_, cx| root.read(cx).anchor.clone());
    let reopen = Rc::new(Cell::new(true));
    let owner = root.downgrade();
    let _subscription = cx.update(|window, cx| {
        window.subscribe(&anchor, cx, move |_, _: &DismissEvent, window, cx| {
            if reopen.replace(false) {
                _ = owner.update(cx, |root, cx| root.show_body(window, cx));
            }
        })
    });
    open(cx);
    cx.simulate_keystrokes("escape");
    settle(cx);
    cx.update(|window, cx| {
        assert!(window.try_find("anchored-body").is_some());
        assert!(root.read(cx).anchor.read(cx).is_open());
        assert!(GlobalState::is_in_deferred_context(cx));
    });
}

#[gpui_kit::test]
fn anchored_overflow_can_replace_native_menu_with_a_chooser_in_the_same_host(
    cx: &mut TestAppContext,
) {
    let (root, cx) = setup(cx);
    root.update(cx, |root, cx| {
        root.native = true;
        cx.notify();
    });
    settle(cx);
    open(cx);
    cx.simulate_keystrokes("down down enter");
    settle(cx);
    cx.update(|window, cx| {
        assert!(window.try_find("popup-menu").is_none());
        assert!(window.try_find("anchored-body").is_some());
        assert!(root.read(cx).anchor.read(cx).is_open());
    });
}

#[gpui_kit::test]
fn anchored_focus_return_includes_popup_controls_outside_its_entry_handle(cx: &mut TestAppContext) {
    let (root, cx) = setup(cx);
    open(cx);
    cx.update(|window, cx| window.focus_next(cx));
    settle(cx);
    let keystroke = Keystroke::parse("enter").unwrap();
    cx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke });
    settle(cx);
    cx.update(|window, cx| {
        assert_eq!(root.read(cx).picked.get(), 1);
        assert!(root.read(cx).anchor.focus_handle(cx).is_focused(window));
    });
}

#[gpui_kit::test]
fn anchored_focus_returns_to_the_native_button_and_keyboard_can_reopen(cx: &mut TestAppContext) {
    let (root, cx) = setup(cx);
    open(cx);
    cx.simulate_keystrokes("escape");
    settle(cx);
    cx.update(|window, cx| {
        assert!(root.read(cx).anchor.focus_handle(cx).is_focused(window));
    });
    let keystroke = Keystroke::parse("enter").unwrap();
    cx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke });
    settle(cx);
    cx.update(|window, _| assert!(window.try_find("anchored-body").is_some()));
}

#[gpui_kit::test]
fn anchored_repeat_pointer_press_closes_instead_of_reopening(cx: &mut TestAppContext) {
    let (root, cx) = setup(cx);
    open(cx);
    let trigger = cx.update(|window, _| window.find("anchored-trigger").bounds());
    cx.simulate_mouse_down(trigger.center(), MouseButton::Left, Default::default());
    settle(cx);
    cx.update(|window, cx| {
        assert!(window.try_find("anchored-body").is_none());
        assert!(!root.read(cx).anchor.read(cx).is_open());
    });
    cx.simulate_mouse_up(trigger.center(), MouseButton::Left, Default::default());
    settle(cx);
    cx.update(|window, _| assert!(window.try_find("anchored-body").is_none()));
}

#[gpui_kit::test]
fn anchored_escape_and_real_outside_pointer_dismiss_and_restore_focus(cx: &mut TestAppContext) {
    let (root, cx) = setup(cx);
    open(cx);
    cx.update(|window, cx| {
        let anchor = root.read(cx).anchor.read(cx);
        assert!(
            anchor
                .content
                .as_ref()
                .unwrap()
                .focus
                .contains_focused(window, cx)
        );
    });
    cx.simulate_keystrokes("escape");
    settle(cx);
    cx.update(|window, cx| {
        assert!(window.try_find("anchored-body").is_none());
        assert!(root.read(cx).anchor.focus_handle(cx).is_focused(window));
    });
    open(cx);
    cx.simulate_click(point(px(10.), px(10.)), Default::default());
    settle(cx);
    cx.update(|window, cx| {
        assert!(window.try_find("anchored-body").is_none());
        assert!(root.read(cx).anchor.focus_handle(cx).is_focused(window));
    });
}

#[gpui_kit::test]
fn anchored_selection_callback_returns_focus_but_respects_an_actions_new_target(
    cx: &mut TestAppContext,
) {
    let (root, cx) = setup(cx);
    open(cx);
    cx.update(|window, cx| window.click("choose-item", cx));
    settle(cx);
    cx.update(|window, cx| {
        assert_eq!(root.read(cx).picked.get(), 1);
        assert!(root.read(cx).anchor.focus_handle(cx).is_focused(window));
        assert!(window.try_find("anchored-body").is_none());
    });
    root.update(cx, |root, _| root.move_focus = true);
    open(cx);
    cx.update(|window, cx| window.click("choose-item", cx));
    settle(cx);
    cx.update(|window, cx| {
        assert_eq!(root.read(cx).picked.get(), 2);
        assert!(root.read(cx).other_focus.is_focused(window));
        assert!(!root.read(cx).anchor.read(cx).is_open());
    });
}

#[gpui_kit::test]
fn anchored_native_menu_keeps_selection_escape_and_repeat_press(cx: &mut TestAppContext) {
    let (root, cx) = setup(cx);
    root.update(cx, |root, cx| {
        root.native = true;
        cx.notify();
    });
    settle(cx);
    open(cx);
    cx.update(|window, _| assert!(window.try_find("popup-menu").is_some()));
    cx.simulate_keystrokes("down enter");
    settle(cx);
    cx.update(|window, cx| {
        assert_eq!(root.read(cx).picked.get(), 1);
        assert!(window.try_find("popup-menu").is_none());
    });
    open(cx);
    let trigger = cx.update(|window, _| window.find("anchored-trigger").bounds());
    cx.simulate_mouse_down(trigger.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(trigger.center(), MouseButton::Left, Default::default());
    settle(cx);
    cx.update(|window, _| assert!(window.try_find("popup-menu").is_none()));
    open(cx);
    cx.simulate_keystrokes("escape");
    settle(cx);
    cx.update(|window, _| assert!(window.try_find("popup-menu").is_none()));
}
