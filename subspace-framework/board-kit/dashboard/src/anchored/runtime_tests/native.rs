use super::{open, settle, setup};
use crate::anchored::dropdown_menu;
use gpui_kit::base::GlobalState;
use gpui_kit::component::{input::InputGroupButton, menu::PopupMenuItem};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Context, IntoElement, KeyDownEvent, KeyUpEvent, Keystroke, Render, TestAppContext, Window,
};
use std::{cell::Cell, rc::Rc};

struct NativeHarness(Rc<Cell<usize>>);

impl Render for NativeHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let picked = self.0.clone();
        dropdown_menu(
            InputGroupButton::new("anchored-trigger")
                .label("Native input-group choices")
                .accessibility_label("Native input-group choices"),
            move |menu, _, _| {
                let picked = picked.clone();
                menu.item(PopupMenuItem::new("Choose item").on_click(move |_, _, _| {
                    picked.set(picked.get() + 1);
                }))
            },
        )
    }
}

#[gpui_kit::test]
fn shared_icon_button_and_replaced_native_menu_keep_trigger_focus(cx: &mut TestAppContext) {
    let (root, cx) = setup(cx);
    root.update(cx, |root, cx| {
        root.native = true;
        crate::icon_button::IconButtonSize::ExtraLarge.apply(cx);
        cx.notify();
    });
    settle(cx);
    open(cx);
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.anchor.update(cx, |anchor, cx| {
                anchor.show_menu_with(window, cx, |menu, _, _| {
                    menu.item(PopupMenuItem::new("Replacement"))
                });
            });
        });
    });
    settle(cx);
    cx.update(|window, _| {
        assert!(window.try_find("popup-menu").is_some());
        assert_eq!(
            window.find("anchored-trigger").bounds().size.height,
            gpui_kit::px(72.)
        );
    });
    cx.simulate_keystrokes("escape");
    settle(cx);
    cx.update(|window, _| {
        assert!(window.try_find("popup-menu").is_none());
        assert_eq!(window.find("anchored-trigger").focused(), Some(true));
    });
    open(cx);
    cx.update(|window, cx| window.click("anchored-trigger", cx));
    settle(cx);
    cx.update(|window, _| assert!(window.try_find("popup-menu").is_none()));
    cx.update(|_, cx| assert!(!root.read(cx).anchor.read(cx).is_open()));
}

#[gpui_kit::test]
fn anchored_input_group_button_preserves_keyboard_activation_and_native_selection(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let picked = Rc::new(Cell::new(0));
    let (_, cx) = cx.add_window_view(|_, _| NativeHarness(picked.clone()));
    settle(cx);
    open(cx);
    cx.simulate_keystrokes("escape");
    settle(cx);
    let keystroke = Keystroke::parse("enter").unwrap();
    cx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke });
    settle(cx);
    cx.update(|window, _| assert!(window.try_find("popup-menu").is_some()));
    cx.simulate_keystrokes("down enter");
    settle(cx);
    assert_eq!(picked.get(), 1);
    cx.update(|window, _| assert!(window.try_find("popup-menu").is_none()));
}

#[gpui_kit::test]
fn anchored_open_native_host_releases_its_registration_when_the_window_closes(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let (_, window) = cx.add_window_view(|_, _| NativeHarness(Rc::default()));
    settle(window);
    open(window);
    window.update(|window, cx| {
        assert!(GlobalState::is_in_deferred_context(cx));
        window.remove_window();
    });
    cx.run_until_parked();
    cx.update(|cx| assert!(!GlobalState::is_in_deferred_context(cx)));
}
