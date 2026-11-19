use super::{FindInPage, Provider};
use crate::rich;
use gpui_kit::base::TextSelectionLayer;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, FocusHandle, Focusable as _, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, TestAppContext,
    Window, div,
};

use crate::find_in_page::test_support::Content;

#[gpui_kit::test]
fn finds_displayed_text_steps_wraps_and_escape_restores_focus(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (pane, cx) = cx.add_window_view(|window, cx| {
        let content = cx.new(|cx| Content {
            focus: cx.focus_handle(),
        });
        let focus = content.read(cx).focus.clone();
        focus.focus(window, cx);
        FindInPage::new(content.into(), Provider::default(), window, cx)
    });
    cx.run_until_parked();
    let original = cx.update(|window, cx| {
        window.render_frame(cx);
        window.focused(cx).unwrap()
    });
    press(crate::secondary!("f"), cx);
    cx.update(|window, cx| {
        assert!(pane.read(cx).session.read(cx).open);
        assert!(pane.read(cx).input.focus_handle(cx).is_focused(window));
        window.input("NeEdLe", cx);
    });
    cx.run_until_parked();
    assert_eq!(
        pane.read_with(cx, |pane, cx| pane.session.read(cx).cursor.position()),
        (1, 3)
    );
    for (key, expected) in [
        ("enter", (2, 3)),
        (crate::secondary!("shift-g"), (1, 3)),
        ("shift-enter", (3, 3)),
    ] {
        press(key, cx);
        assert_eq!(
            pane.read_with(cx, |pane, cx| pane.session.read(cx).cursor.position()),
            expected
        );
    }
    press("escape", cx);
    cx.update(|window, cx| {
        assert!(!pane.read(cx).session.read(cx).open);
        assert_eq!(pane.read(cx).session.read(cx).cursor.position(), (0, 0));
        assert!(original.is_focused(window));
    });
}
fn press(key: &str, cx: &mut gpui_kit::VisualTestContext) {
    cx.update(|window, cx| window.press(key, cx));
    cx.run_until_parked();
}

struct LongContent {
    focus: FocusHandle,
    scroll: gpui_kit::ScrollHandle,
}
impl Render for LongContent {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus)
            .size_full()
            .child(TextSelectionLayer)
            .child(crate::find_in_page::scroll(
                self.scroll.clone(),
                div()
                    .id("long-literal-scroll")
                    .h(gpui_kit::px(120.))
                    .w(gpui_kit::px(400.))
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(rich::plain_text(
                        "long-literal",
                        &format!("{}unique-end", "ordinary line\r\n".repeat(80)),
                    )),
            ))
    }
}
#[gpui_kit::test]
fn literal_find_reveals_an_offscreen_range_in_its_own_container(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let scroll = gpui_kit::ScrollHandle::new();
    let (pane, cx) = cx.add_window_view(|window, cx| {
        let content = cx.new(|cx| LongContent {
            focus: cx.focus_handle(),
            scroll: scroll.clone(),
        });
        let focus = content.read(cx).focus.clone();
        focus.focus(window, cx);
        FindInPage::new(content.into(), Provider::default(), window, cx)
    });
    cx.run_until_parked();
    press(crate::secondary!("f"), cx);
    cx.update(|window, cx| window.input("unique-end", cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(pane.read(cx).session.read(cx).cursor.position(), (1, 1));
        assert!(
            scroll.offset().y < gpui_kit::px(0.),
            "the literal match must reveal in its own scroll owner"
        );
    });
}

#[path = "tests/actions.rs"]
mod action_routing;
#[path = "tests/live.rs"]
mod live;
#[path = "tests/routing.rs"]
mod routing;
#[path = "tests/scope.rs"]
mod scope;
