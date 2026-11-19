use super::{IconButton, IconButtonSize};
use gpui_kit::component::{Disableable as _, button::Button, h_flex};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, StatefulInteractiveElement as _, TestAppContext,
    TestSupportExt as _, Window, div, px, size,
};
use std::{cell::Cell, rc::Rc};
use subspace_lucide::IconName;

struct Fixture {
    outside: FocusHandle,
    enabled: bool,
    loading: bool,
    expanded: bool,
    calls: usize,
    native_calls: Rc<Cell<usize>>,
}

impl Render for Fixture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let native_calls = self.native_calls.clone();
        h_flex()
            .child(
                div()
                    .id("outside")
                    .test_support()
                    .track_focus(&self.outside),
            )
            .child(
                IconButton::new("disclosure")
                    .icon(IconName::ChevronDown)
                    .label("Details")
                    .focus_on_click()
                    .aria_expanded(self.expanded)
                    .disabled(!self.enabled)
                    .loading(self.loading)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.expanded = !this.expanded;
                        this.calls += 1;
                        cx.notify();
                    })),
            )
            .child(
                IconButton::new("ordinary")
                    .icon(IconName::Settings)
                    .accessibility_label("Settings"),
            )
            .child(
                IconButton::from_button(Button::new("native-template").on_click(move |_, _, _| {
                    native_calls.set(native_calls.get() + 1);
                }))
                .icon(IconName::X)
                .label("Native action")
                .focus_on_click(),
            )
    }
}

fn fixture(cx: &mut Context<Fixture>) -> Fixture {
    Fixture {
        outside: cx.focus_handle(),
        enabled: true,
        loading: false,
        expanded: false,
        calls: 0,
        native_calls: Rc::default(),
    }
}

#[gpui_kit::test]
fn opt_in_pointer_focus_survives_rebuild_and_preserves_native_callbacks(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let handle = cx.open_window(size(px(1000.), px(300.)), |_, cx| fixture(cx));
    for choice in [IconButtonSize::Small, IconButtonSize::ExtraLarge] {
        cx.update(|cx| choice.apply(cx));
        cx.update_window(*handle, |_, window, cx| {
            window.click("disclosure", cx);
            assert_eq!(window.find("disclosure").focused(), Some(true));
            assert_eq!(window.find("disclosure").expanded(), Some(true));
            window.press("space", cx);
            assert_eq!(window.find("disclosure").expanded(), Some(false));
            window.press("enter", cx);
            assert_eq!(window.find("disclosure").expanded(), Some(true));
            window.press("space", cx);
            assert_eq!(window.find("disclosure").expanded(), Some(false));
        })
        .unwrap();
    }
    assert_eq!(handle.read_with(cx, |this, _| this.calls).unwrap(), 8);
    cx.update_window(*handle, |_, window, cx| {
        window.click("native-template", cx);
        assert_eq!(window.find("native-template").focused(), Some(true));
        window.press("space", cx);
    })
    .unwrap();
    assert_eq!(
        handle
            .read_with(cx, |this, _| this.native_calls.get())
            .unwrap(),
        2
    );
}

#[gpui_kit::test]
fn opt_in_focus_is_suppressed_when_inert_and_ordinary_toolbar_focus_is_unchanged(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let handle = cx.open_window(size(px(1000.), px(300.)), |_, cx| fixture(cx));
    for (enabled, loading) in [(false, false), (true, true), (true, false)] {
        handle
            .update(cx, |this, window, cx| {
                this.enabled = enabled;
                this.loading = loading;
                this.outside.focus(window, cx);
                cx.notify();
            })
            .unwrap();
        cx.update_window(*handle, |_, window, cx| {
            let id = if enabled && !loading {
                "ordinary"
            } else {
                "disclosure"
            };
            window.click(id, cx);
            assert_ne!(window.find(id).focused(), Some(true));
        })
        .unwrap();
        handle
            .update(cx, |this, window, _| {
                assert!(this.outside.is_focused(window));
                assert_eq!(this.calls, 0);
            })
            .unwrap();
    }
}
