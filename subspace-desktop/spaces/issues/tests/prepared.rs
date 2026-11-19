use gpui_kit::base::{TextSelection, TextSelectionLayer};
use gpui_kit::{
    Context, IntoElement, Modifiers, MouseButton, ParentElement as _, Render, SharedString,
    Styled as _, TestAppContext, Window, div, point, px,
};
use subspace_framework::dashboard::rich;
use subspace_issues_board::markdown::{Options, prepare_markdown};
struct PreparedFixture(SharedString);
impl Render for PreparedFixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(TextSelectionLayer)
            .child(div().w(px(640.)).child(rich::rich_text_prepared(
                "prepared",
                self.0.clone(),
                |_, _, _| true,
            )))
    }
}
#[gpui_kit::test]
fn prepared_media_remains_readable_in_the_shared_markdown_renderer(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let prepared: SharedString = prepare_markdown(
        "Price $5. ![Diagram](nett-media:fixture-image)",
        &Options::new(&[], &[]),
    )
    .into();
    let (_, cx) = cx.add_window_view(|_, _| PreparedFixture(prepared.clone()));
    cx.run_until_parked();
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    cx.simulate_mouse_down(
        point(px(40.), px(12.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        point(px(40.), px(12.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.update(|window, cx| window.dispatch_action(Box::new(gpui_kit::base::input::SelectAll), cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        let _ = window.draw(cx);
        assert_eq!(
            TextSelection::selected_text(window, cx).trim(),
            "Price $5. Attachment unavailable"
        );
    });
}
