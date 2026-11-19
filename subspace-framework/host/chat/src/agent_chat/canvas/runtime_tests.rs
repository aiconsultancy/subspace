use super::{ChatPanel, Tokens};
use crate::agent_chat::test_menu;
use gpui_kit::component::v_flex;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, VisualTestContext, Window, px,
};
use std::sync::Arc;
use subspace_agent::canvas::{Canvas, CanvasContent, CanvasKind};
use subspace_dashboard::icon_button::IconButtonSize;

struct Chrome {
    chat: Entity<ChatPanel>,
    canvas: Arc<Canvas>,
}

impl Render for Chrome {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let (header, canvas) = self.chat.update(cx, |chat, cx| {
            (
                chat.render_header(&tokens, cx),
                chat.canvas_header(&self.canvas, &tokens, cx),
            )
        });
        v_flex().w(px(300.)).child(header).child(canvas)
    }
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
}

fn setup(cx: &mut TestAppContext) -> (Entity<ChatPanel>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_global(IconButtonSize::ExtraLarge);
    });
    let (root, cx) = cx.add_window_view(|window, cx| {
        let canvas = Arc::new(Canvas {
            id: "chrome-test-canvas".into(),
            kind: CanvasKind::Diagram,
            title: "A long test canvas title".into(),
            caption: "Test".into(),
            opened_at: "test".into(),
            source: String::new(),
            content: CanvasContent::Error("Test content".into()),
        });
        let chat = cx.new(|cx| ChatPanel::new_copilot(window, cx));
        chat.update(cx, |chat, _| {
            chat.tab_mut().canvases.items.push(canvas.clone())
        });
        Chrome { chat, canvas }
    });
    let chat = root.read_with(cx, |root, _| root.chat.clone());
    settle(cx);
    (chat, cx)
}

#[gpui_kit::test]
fn narrow_extra_large_headers_grow_and_overflow_without_wrapping(cx: &mut TestAppContext) {
    let (_, cx) = setup(cx);
    cx.update(|window, _| {
        // Canvas owns a one-pixel bottom divider. Flex aligns controls in
        // the interior above it, not in the border box's centre.
        for (id, bottom_border) in [("chat-header", px(0.)), ("canvas-header", px(1.))] {
            let header = window.find(id).bounds();
            let more = window.within(id).find("overflow-toolbar-button").bounds();
            assert!(header.size.height >= px(72.));
            assert!(more.left() >= header.left() && more.right() <= header.right());
            assert!(more.top() >= header.top() && more.bottom() <= header.bottom());
            let interior_center = (header.top() + header.bottom() - bottom_border) / 2.;
            assert_eq!(more.center().y, interior_center);
        }
        assert_eq!(
            window.find(("chat-tab-close", 0usize)).bounds().size.height,
            px(20.)
        );
        assert!(window.try_find("canvas-hide").is_none());
    });
}

#[gpui_kit::test]
fn narrow_header_and_canvas_overflow_keep_their_actions(cx: &mut TestAppContext) {
    let (chat, cx) = setup(cx);
    cx.update(|window, cx| {
        window
            .within("chat-header")
            .click("overflow-toolbar-button", cx)
    });
    cx.update(|window, cx| test_menu::click(window, "Sessions", cx));
    settle(cx);
    chat.read_with(cx, |chat, _| assert!(chat.sessions_open));
    cx.update(|window, cx| {
        window
            .within("canvas-header")
            .click("overflow-toolbar-button", cx)
    });
    cx.update(|window, cx| test_menu::click(window, "Hide canvas area", cx));
    settle(cx);
    chat.read_with(cx, |chat, _| {
        assert!(chat.tab().canvases.hidden);
        assert!(!chat.tab().canvases.gallery);
        assert_eq!(chat.tabs.len(), 1);
        assert_eq!(chat.tabs[0].id, 1);
    });
}
