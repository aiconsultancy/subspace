use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Context, IntoElement, ParentElement as _, Render, Role, Styled as _, TestAppContext, Window,
    div, px, size,
};
use subspace_lucide::IconName;

use super::{SurfaceHeader, close_tooltip};
use crate::action::{Intent, action_button};

struct Fixture {
    closed: Rc<Cell<usize>>,
}

impl Render for Fixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let closed = self.closed.clone();
        div().size_full().child(
            SurfaceHeader::new(
                IconName::FileText,
                "A surface title long enough to need truncating in a narrow window",
                "Close fixture",
                move |_, _, _| closed.set(closed.get() + 1),
            )
            .description("What the surface is for.")
            .action(action_button(
                "fixture-action",
                Intent::Neutral,
                Some(IconName::Database),
                "Sources",
            )),
        )
    }
}

#[test]
fn the_close_tooltip_names_the_escape_key() {
    assert_eq!(close_tooltip("Close logs"), "Close logs (Esc)");
}

#[gpui_kit::test]
fn close_stays_visible_after_the_actions_in_a_narrow_window(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let closed = Rc::new(Cell::new(0));
    let fixture = closed.clone();
    let (_, cx) = cx.add_window_view(move |_, _| Fixture { closed: fixture });
    cx.simulate_resize(size(px(240.), px(200.)));
    cx.update(|window, cx| {
        window.render_frame(cx);
        let header = window.find("surface-header").bounds();
        let action = window.find("fixture-action").bounds();
        let close = window.find("surface-close");
        assert_eq!(close.role(), Some(Role::Button));
        assert_eq!(close.label(), Some("Close fixture"));
        let close = close.bounds();
        assert!(close.size.width > px(0.));
        assert!(action.right() <= close.left());
        assert!(close.right() <= header.right());
        assert!(window.find("surface-title").bounds().right() <= action.left());
        window.click("surface-close", cx);
    });
    assert_eq!(closed.get(), 1);
}
