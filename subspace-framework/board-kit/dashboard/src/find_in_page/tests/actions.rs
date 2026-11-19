//! Content Escape actions give Find first refusal, then retain their old behaviour.
use super::super::Close;
use super::{FindInPage, Provider, press};
use gpui_kit::{
    AppContext as _, Context, FocusHandle, InteractiveElement as _, IntoElement, KeyBinding,
    ParentElement as _, Render, Styled as _, TestAppContext, Window, actions, div,
};
use std::{cell::Cell, rc::Rc};
actions!(find_test_owner, [Leave, CloseEditor]);
struct Owner {
    modal: bool,
    focus: FocusHandle,
    editor: FocusHandle,
    leaves: Rc<Cell<usize>>,
    editor_closes: Rc<Cell<usize>>,
}
impl Render for Owner {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .key_context("FindTestOwner")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Leave, _, _| this.leaves.set(this.leaves.get() + 1)))
            .child(
                div()
                    .size_full()
                    .key_context(if self.modal {
                        "FindTestEditor SubspaceModal"
                    } else {
                        "FindTestEditor"
                    })
                    .track_focus(&self.editor)
                    .on_action(cx.listener(|this, _: &CloseEditor, _, _| {
                        this.editor_closes.set(this.editor_closes.get() + 1)
                    })),
            )
    }
}
#[gpui_kit::test]
fn owner_escape_yields_to_find_and_deeper_editors_retain_their_own_escape(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let leaves = Rc::new(Cell::new(0));
    let editors = Rc::new(Cell::new(0));
    let mut owner = None;
    let (pane, cx) = cx.add_window_view(|window, cx| {
        cx.bind_keys([
            KeyBinding::new("escape", Leave, Some("FindTestOwner")),
            KeyBinding::new("escape", Close, Some("FindTestOwner")),
            KeyBinding::new("escape", CloseEditor, Some("FindTestEditor")),
        ]);
        let content = cx.new(|cx| Owner {
            modal: false,
            focus: cx.focus_handle(),
            editor: cx.focus_handle(),
            leaves: leaves.clone(),
            editor_closes: editors.clone(),
        });
        content.read(cx).focus.clone().focus(window, cx);
        owner = Some(content.clone());
        FindInPage::new(content.into(), Provider::default(), window, cx)
    });
    let owner = owner.unwrap();
    cx.run_until_parked();
    press(crate::secondary!("f"), cx);
    cx.update(|window, cx| owner.read(cx).editor.clone().focus(window, cx));
    press("escape", cx);
    assert_eq!(editors.get(), 1);
    assert!(pane.read_with(cx, |p, cx| p.session.read(cx).open));
    cx.update(|window, cx| owner.read(cx).focus.clone().focus(window, cx));
    press("escape", cx);
    assert_eq!(leaves.get(), 0);
    assert!(!pane.read_with(cx, |p, cx| p.session.read(cx).open));
    press("escape", cx);
    assert_eq!(leaves.get(), 1);
}

#[gpui_kit::test]
fn another_contexts_popover_does_not_close_content_behind_find(cx: &mut TestAppContext) {
    let (pane, cx) = crate::find_in_page::test_support::mount(cx);
    press(crate::secondary!("f"), cx);
    let _other_popover =
        cx.update(|_, cx| gpui_kit::base::GlobalState::register_deferred_popover(cx));
    press("escape", cx);
    assert!(!pane.read_with(cx, |p, cx| p.session.read(cx).open));
}
#[gpui_kit::test]
fn a_modal_keeps_escape_and_does_not_open_find_under_it(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut owner = None;
    let (pane, cx) = cx.add_window_view(|window, cx| {
        cx.bind_keys([KeyBinding::new(
            "escape",
            CloseEditor,
            Some("FindTestEditor"),
        )]);
        let content = cx.new(|cx| Owner {
            modal: true,
            focus: cx.focus_handle(),
            editor: cx.focus_handle(),
            leaves: Rc::new(Cell::new(0)),
            editor_closes: Rc::new(Cell::new(0)),
        });
        content.read(cx).editor.clone().focus(window, cx);
        owner = Some(content.clone());
        FindInPage::new(content.into(), Provider::default(), window, cx)
    });
    let owner = owner.unwrap();
    cx.run_until_parked();
    press(crate::secondary!("f"), cx);
    assert!(!pane.read_with(cx, |p, cx| p.session.read(cx).open));
    press("escape", cx);
    assert_eq!(owner.read_with(cx, |o, _| o.editor_closes.get()), 1);
}
