//! Copy/chat use the exact native range and do not pick up an unrelated block.
use super::{Format, rich_text};
use gpui_kit::base::{TextSelection, TextSelectionLayer};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Context, IntoElement, Modifiers, MouseButton, ParentElement as _, Pixels, Point, Render,
    Styled as _, TestAppContext, VisualTestContext, Window, div, point, px,
};
struct Fixture;
impl Render for Fixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(TextSelectionLayer)
            .child(div().w(px(500.)).child(rich_text(
                "payload-range",
                Format::Markdown,
                "alpha bravo charlie delta echo",
            )))
    }
}
fn selected(cx: &mut TestAppContext) -> &mut VisualTestContext {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|_, _| Fixture);
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    let from = caret("alpha ", cx);
    let to = caret("alpha bravo charlie delta", cx);
    drag(from, to, cx);
    cx
}
fn drag(from: Point<Pixels>, to: Point<Pixels>, cx: &mut VisualTestContext) {
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
}
fn caret(prefix: &str, cx: &mut VisualTestContext) -> Point<Pixels> {
    let bounds = cx.update(|window, _| window.find("rich-menu-Name(\"payload-range\")").bounds());
    let start = point(bounds.left() + px(0.01), bounds.center().y);
    let (mut left, mut right) = (start.x, bounds.right() - px(0.01));
    for _ in 0..16 {
        let position = point((left + right) / 2., start.y);
        drag(start, position, cx);
        let value = cx
            .update(TextSelection::selected_text)
            .trim_end_matches('\n')
            .to_owned();
        if value == prefix {
            return position;
        }
        if value.len() < prefix.len() {
            left = position.x;
        } else {
            right = position.x;
        }
    }
    panic!("native caret for {prefix:?} not found");
}
fn menu(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.right_click("rich-menu-Name(\"payload-range\")", cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
}
#[gpui_kit::test]
fn copy_uses_exact_middle_words_without_a_synthetic_paragraph_newline(cx: &mut TestAppContext) {
    let cx = selected(cx);
    menu(cx);
    cx.update(|window, cx| {
        window.within("popup-menu").click(0usize, cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("bravo charlie delta")
        );
    });
}
#[gpui_kit::test]
fn opening_a_menu_sets_no_chat_payload_but_explicit_add_does(cx: &mut TestAppContext) {
    let cx = selected(cx);
    cx.update(|_, cx| assert!(crate::take_selection(cx).is_empty()));
    menu(cx);
    cx.update(|window, cx| {
        assert!(crate::take_selection(cx).is_empty());
        assert_eq!(
            window.within("popup-menu").find(2usize).label(),
            Some("Add to chat")
        );
        window.within("popup-menu").click(2usize, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(crate::take_selection(cx), "bravo charlie delta"));
}
