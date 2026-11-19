//! Reopening a retained query uses the current page's native registrations.
use super::{FindInPage, Provider, press};
use crate::{find_in_page, rich};
use gpui_kit::base::TextSelectionLayer;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _,
    Styled as _, TestAppContext, Window, div, px,
};
use std::{cell::RefCell, rc::Rc};
struct Page {
    focus: FocusHandle,
    identity: Rc<RefCell<SharedString>>,
    scroll: ScrollHandle,
}
impl Render for Page {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        find_in_page::identified(
            self.identity.borrow().clone(),
            true,
            div()
                .size_full()
                .track_focus(&self.focus)
                .child(TextSelectionLayer)
                .child(find_in_page::scroll(
                    self.scroll.clone(),
                    div()
                        .id("scope-scroll")
                        .h(px(120.))
                        .w(px(400.))
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll)
                        .child(rich::plain_text(
                            "page-body",
                            &format!("{}needle end", "ordinary line\n".repeat(80)),
                        )),
                )),
            cx,
        )
    }
}
#[gpui_kit::test]
fn reopening_on_another_page_reveals_the_retained_query(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let identity = Rc::new(RefCell::new(SharedString::from("issue-a")));
    let read = identity.clone();
    let scroll = ScrollHandle::new();
    let mut page = None;
    let (pane, cx) = cx.add_window_view(|window, cx| {
        let content = cx.new(|cx| Page {
            focus: cx.focus_handle(),
            identity: identity.clone(),
            scroll: scroll.clone(),
        });
        content.read(cx).focus.clone().focus(window, cx);
        page = Some(content.clone());
        FindInPage::new(
            content.into(),
            Provider {
                snapshot: Rc::new(move |_| (read.borrow().clone(), Vec::new())),
                ..Default::default()
            },
            window,
            cx,
        )
    });
    cx.run_until_parked();
    press(crate::secondary!("f"), cx);
    cx.update(|window, cx| window.input("needle", cx));
    cx.run_until_parked();
    assert!(scroll.offset().y < px(0.));
    press("escape", cx);
    *identity.borrow_mut() = "issue-b".into();
    scroll.set_offset(Default::default());
    page.unwrap().update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    press(crate::secondary!("f"), cx);
    cx.update(|window, cx| window.render_frame(cx));
    cx.run_until_parked();
    assert_eq!(
        pane.read_with(cx, |p, cx| p.session.read(cx).cursor.position()),
        (1, 1)
    );
    assert!(
        scroll.offset().y < px(0.),
        "opening Find on page B must reveal its existing match"
    );
}
struct Columns {
    focus: FocusHandle,
}
impl gpui_kit::Focusable for Columns {
    fn focus_handle(&self, _: &gpui_kit::App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for Columns {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui_kit::component::h_flex()
            .size_full()
            .track_focus(&self.focus)
            .child(TextSelectionLayer)
            .child(
                gpui_kit::component::v_flex().w(px(400.)).children([
                    rich::plain_text(
                        "column-description",
                        &format!("needle start\n{}needle end", "ordinary line\n".repeat(80)),
                    )
                    .into_any_element(),
                    rich::plain_text("column-comment", "needle comment").into_any_element(),
                ]),
            )
            .child(
                div()
                    .w(px(200.))
                    .child(rich::plain_text("column-property", "needle property")),
            )
    }
}
#[gpui_kit::test]
fn columns_follow_document_order_instead_of_interleaving_their_origins(cx: &mut TestAppContext) {
    let (pane, cx) = crate::find_in_page::test_support::mount_with(cx, |cx| Columns {
        focus: cx.focus_handle(),
    });
    press(crate::secondary!("f"), cx);
    cx.update(|window, cx| window.input("needle", cx));
    cx.run_until_parked();
    pane.read_with(cx, |p, cx| {
        let actual: Vec<_> = p
            .session
            .read(cx)
            .cursor
            .hits()
            .iter()
            .map(|hit| hit.document.to_string())
            .collect();
        let expected: Vec<_> = [
            "column-description",
            "column-description",
            "column-comment",
            "column-property",
        ]
        .map(|id| format!("{:?}", gpui_kit::ElementId::from(id)))
        .into();
        assert_eq!(
            actual, expected,
            "finish each text column before moving to the next column"
        );
    });
}
