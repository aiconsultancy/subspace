use crate::{
    AddToChat,
    icon_button::IconButtonSize,
    list::ServiceLink,
    panel::{self, PanelActions, PanelHeader},
};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, TestSupportExt as _, VisualTestContext, Window, div, px, size,
};
use std::{cell::Cell, rc::Rc};

struct Fixture {
    closed: Rc<Cell<usize>>,
    steps: Rc<Cell<isize>>,
    chats: usize,
}

impl Render for Fixture {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let closed = self.closed.clone();
        let steps = self.steps.clone();
        div()
            .size_full()
            .flex()
            .justify_end()
            .on_action(cx.listener(|this, _: &AddToChat, _, _| this.chats += 1))
            .child(panel::frame(
                PanelHeader {
                    kind: "Pull request".into(),
                    id: "long-identifier-keeps-its-title-and-actions".into(),
                    link: Some(ServiceLink::new(
                        "Open in GitHub",
                        "https://example.invalid/item",
                    )),
                    ..Default::default()
                },
                div().id("panel-content").test_support().child("Body"),
                PanelActions::new(
                    move |_, _| closed.set(closed.get() + 1),
                    Some(Rc::new(move |step, _, _| steps.set(steps.get() + step))),
                ),
                window,
                cx,
            ))
    }
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..6 {
        cx.update(|window, cx| window.render_frame(cx));
        cx.run_until_parked();
    }
}

fn item(window: &mut Window, label: &str) -> usize {
    let menu = window.within("popup-menu");
    (0usize..16)
        .find(|index| {
            menu.try_find(*index)
                .is_some_and(|item| item.label() == Some(label))
        })
        .unwrap_or_else(|| panic!("missing popup item {label}"))
}

#[gpui_kit::test]
fn xl_panel_header_keeps_close_and_body_visible_with_single_row_overflow_actions(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        IconButtonSize::ExtraLarge.apply(cx);
    });
    let closed = Rc::new(Cell::new(0));
    let steps = Rc::new(Cell::new(0));
    let (view, cx) = cx.add_window_view(|_, _| Fixture {
        closed: closed.clone(),
        steps: steps.clone(),
        chats: 0,
    });
    cx.simulate_resize(size(px(420.), px(600.)));
    settle(cx);
    cx.update(|window, cx| {
        let toolbar = window.find("panel-toolbar").bounds();
        let close = window.find("panel-close").bounds();
        let trigger = window.find("overflow-toolbar-button").bounds();
        // One line: the 72 px button, its 3 px focus ring above and below,
        // and the hairline.
        assert!(toolbar.size.height <= px(79.));
        assert_eq!(close.size.height, px(72.));
        assert_eq!(close.center().y, trigger.center().y);
        // Close is the last control and never moves into overflow.
        assert!(trigger.right() <= close.left() && close.right() <= toolbar.right());
        assert!(trigger.left() >= toolbar.left());
        assert!(window.find("panel-content").bounds().top() >= toolbar.bottom());
        window.click("overflow-toolbar-button", cx);
        let next = item(window, "Next item");
        window.within("popup-menu").click(next, cx);
    });
    assert_eq!(steps.get(), 1);
    settle(cx);
    cx.update(|window, cx| {
        window.click("overflow-toolbar-button", cx);
        let chat = item(window, "Add to chat");
        window.within("popup-menu").click(chat, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(view.read(cx).chats, 1);
        window.click("panel-close", cx);
    });
    assert_eq!(closed.get(), 1);
}
