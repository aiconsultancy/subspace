use super::RichTextContext;
use gpui_kit::base::{TextSelectionLayer, TextView, TextViewState};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Styled as _, TestAppContext, TestSupportExt as _, Window, div, px,
};
use std::sync::Arc;
struct MenuFixture(Box<dyn Fn() -> AnyElement>);
impl Render for MenuFixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(TextSelectionLayer)
            .child((self.0)())
    }
}
#[gpui_kit::test]
fn inline_context_uses_its_specific_fallback_without_changing_native_text(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let state = cx.update(|cx| cx.new(|cx| TextViewState::markdown("KEY is still loading", cx)));
    let (_, cx) = cx.add_window_view(|_, _| {
        MenuFixture(Box::new(move || {
            let context = RichTextContext {
                state: state.downgrade(),
                target: Arc::default(),
                fallback: None,
                highlights: Vec::new(),
            };
            context
                .text_menu(
                    div()
                        .id("inline-menu")
                        .test_support()
                        .w(px(400.))
                        .child(TextView::new(&state)),
                    "KEY",
                    crate::row_menu::MenuChat::Off,
                )
                .into_any_element()
        }))
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.right_click("inline-menu", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.within("popup-menu").click(0usize, cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("KEY")
        );
    });
}
#[gpui_kit::test]
fn custom_inline_find_highlights_map_native_source_to_label_offsets(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let state = cx.update(|cx| cx.new(|cx| TextViewState::markdown("head [bravo](url) tail", cx)));
    cx.run_until_parked();
    cx.update(|cx| {
        let color = crate::Tokens::current(cx).accent;
        let context = RichTextContext {
            state: state.downgrade(),
            target: Arc::default(),
            fallback: None,
            highlights: vec![(6..9, color)],
        };
        let spans = context.source_highlights(5..17, cx);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].0, 1..4);
        assert_eq!(spans[0].1.background_color, Some(color));
    });
}
#[gpui_kit::test]
fn legacy_text_menu_with_keeps_copy_payload_and_explicit_chat_opt_out(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|_, _| {
        MenuFixture(Box::new(|| {
            crate::rich::text_menu_with(
                div()
                    .id("legacy-menu")
                    .test_support()
                    .w(px(300.))
                    .h(px(40.))
                    .child("displayed block"),
                "displayed block",
                crate::row_menu::MenuChat::Off,
            )
            .into_any_element()
        }))
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.right_click("legacy-menu", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.within("popup-menu").try_find(1usize).is_none());
        window.within("popup-menu").click(0usize, cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("displayed block")
        );
    });
}
