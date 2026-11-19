use super::settle;
use crate::overflow_toolbar::{OverflowToolbar, OverflowUnit, PopupEntry};
use gpui_kit::component::{button::Button, menu::PopupMenuItem};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement as _, Render, Styled as _, TestAppContext, Window,
    div, px,
};
use std::{cell::RefCell, rc::Rc};

struct Owner {
    toolbar: Option<Entity<OverflowToolbar>>,
    revision: usize,
    disabled: bool,
    calls: Rc<RefCell<Vec<usize>>>,
}

impl Render for Owner {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let revision = self.revision;
        let disabled = self.disabled;
        let calls = self.calls.clone();
        let unit = OverflowUnit::inline(
            "choice",
            100.,
            |_, _| Button::new("inline-choice").label("Choice").w(px(100.)),
            vec![PopupEntry::new(move || {
                let calls = calls.clone();
                PopupMenuItem::new("Choice")
                    .disabled(disabled)
                    .on_click(move |_, _, _| calls.borrow_mut().push(revision))
            })],
        );
        let toolbar = OverflowToolbar::sync(&mut self.toolbar, vec![unit], cx);
        div().w(px(40.)).child(toolbar)
    }
}

#[gpui_kit::test]
fn sync_per_render_preserves_native_selection_and_uses_latest_callback_and_disabled_state(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let (owner, cx) = cx.add_window_view(|_, _| Owner {
        toolbar: None,
        revision: 0,
        disabled: false,
        calls: calls.clone(),
    });
    settle(cx);
    cx.update(|window, cx| {
        window.click("overflow-toolbar-button", cx);
        window.press("down", cx);
    });
    owner.update(cx, |owner, cx| {
        owner.revision = 1;
        cx.notify();
    });
    settle(cx);
    cx.update(|window, cx| window.press("enter", cx));
    assert_eq!(
        *calls.borrow(),
        vec![1],
        "same-label callback must be fresh without resetting navigation"
    );
    cx.update(|window, cx| window.click("overflow-toolbar-button", cx));
    owner.update(cx, |owner, cx| {
        owner.disabled = true;
        cx.notify();
    });
    settle(cx);
    cx.update(|window, cx| {
        window.press("down", cx);
        window.press("enter", cx);
    });
    assert_eq!(
        *calls.borrow(),
        vec![1],
        "new disabled state must reject native keyboard confirm"
    );
}
