//! Focused child views own their keys before an enclosing find surface.
use super::super::{FindInPage, Provider};
use super::{Content, press};
use gpui_kit::{
    AppContext as _, Context, FocusHandle, InteractiveElement as _, IntoElement, Render,
    Styled as _, TestAppContext, Window, div,
};
use std::{cell::RefCell, rc::Rc};
struct Raw {
    focus: FocusHandle,
    keys: Rc<RefCell<Vec<String>>>,
}
impl Render for Raw {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let keys = self.keys.clone();
        div()
            .size_full()
            .track_focus(&self.focus)
            .key_context(crate::RAW_KEYS_CONTEXT)
            .on_key_down(move |event, _, _| keys.borrow_mut().push(event.keystroke.key.clone()))
    }
}
#[gpui_kit::test]
fn raw_child_keeps_find_keys_and_escape_even_when_find_is_open(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let keys = Rc::new(RefCell::new(Vec::new()));
    let (pane, cx) = cx.add_window_view(|window, cx| {
        let raw = cx.new(|cx| Raw {
            focus: cx.focus_handle(),
            keys: keys.clone(),
        });
        let focus = raw.read(cx).focus.clone();
        focus.focus(window, cx);
        FindInPage::new(raw.into(), Provider::default(), window, cx)
    });
    cx.run_until_parked();
    for key in [
        crate::secondary!("f"),
        crate::secondary!("g"),
        crate::secondary!("shift-g"),
    ] {
        press(key, cx);
    }
    assert!(!pane.read_with(cx, |p, cx| p.session.read(cx).open));
    assert_eq!(keys.borrow().len(), 3);
    cx.update(|window, cx| {
        pane.update(cx, |p, cx| p.show(window, cx));
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let focus = pane.read(cx).return_to.clone().unwrap();
        focus.focus(window, cx);
    });
    press("escape", cx);
    assert!(pane.read_with(cx, |p, cx| p.session.read(cx).open));
    assert_eq!(keys.borrow().last().unwrap(), "escape");
}
#[gpui_kit::test]
fn escape_closes_the_focused_nested_find_before_its_outer_surface(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut inner = None;
    let (outer, cx) = cx.add_window_view(|window, cx| {
        let content = cx.new(|cx| Content {
            focus: cx.focus_handle(),
        });
        content.read(cx).focus.clone().focus(window, cx);
        let pane = cx.new(|cx| FindInPage::new(content.into(), Provider::default(), window, cx));
        inner = Some(pane.clone());
        FindInPage::new(pane.into(), Provider::default(), window, cx)
    });
    let inner = inner.unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        outer.update(cx, |p, cx| p.show(window, cx));
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        inner.update(cx, |p, cx| p.show(window, cx));
    });
    cx.run_until_parked();
    press("escape", cx);
    assert!(!inner.read_with(cx, |p, cx| p.session.read(cx).open));
    assert!(outer.read_with(cx, |p, cx| p.session.read(cx).open));
}

#[gpui_kit::test]
fn closed_nested_find_propagates_close_to_the_open_ancestor(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut content_focus = None;
    let (outer, cx) = cx.add_window_view(|window, cx| {
        let content = cx.new(|cx| Content {
            focus: cx.focus_handle(),
        });
        let focus = content.read(cx).focus.clone();
        focus.focus(window, cx);
        content_focus = Some(focus);
        let inner = cx.new(|cx| FindInPage::new(content.into(), Provider::default(), window, cx));
        FindInPage::new(inner.into(), Provider::default(), window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| outer.update(cx, |p, cx| p.show(window, cx)));
    cx.run_until_parked();
    cx.update(|window, cx| content_focus.as_ref().unwrap().focus(window, cx));
    press("escape", cx);
    assert!(!outer.read_with(cx, |p, cx| p.session.read(cx).open));
}
