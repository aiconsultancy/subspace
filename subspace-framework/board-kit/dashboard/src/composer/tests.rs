use super::{Composer, ComposerEvent};
use gpui_kit::EntityInputHandler as _;
use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use std::{cell::RefCell, rc::Rc};

#[gpui_kit::test]
fn retained_composer_keeps_multiline_text_on_escape_and_freezes_a_post(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(crate::bind_keys);
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = Composer::new("New comment", "Leave a comment", window, cx);
        editor.set_retained(true, cx);
        editor
    });
    let (events, _sub) = observe(&editor, cx);
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| editor.focus(window, cx));
        window.render_frame(cx);
    });
    cx.update(|window, cx| {
        window.input("First", cx);
        window.press("enter", cx);
        window.input("Second", cx);
    });
    cx.run_until_parked();
    assert!(events.borrow().is_empty(), "Enter inserts a newline");
    cx.update(|window, cx| window.press("escape", cx));
    cx.run_until_parked();
    assert_eq!(&*events.borrow(), &[ComposerEvent::Cancel]);
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            assert_eq!(editor.text(cx), "First\nSecond");
            editor.focus(window, cx);
        });
        window.press(crate::secondary!("enter"), cx);
    });
    cx.run_until_parked();
    assert!(
        matches!(events.borrow().last(), Some(ComposerEvent::Submit(text)) if text == "First\nSecond")
    );
    let count = events.borrow().len();
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| editor.set_busy(true, cx));
        window.render_frame(cx);
        window.input("Do not change", cx);
        window.press(crate::secondary!("enter"), cx);
        window.click("composer-submit", cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(editor.read(cx).text(cx), "First\nSecond"));
    assert_eq!(events.borrow().len(), count);
}

#[gpui_kit::test]
fn composition_must_finish_before_submit(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(crate::bind_keys);
    let (editor, cx) =
        cx.add_window_view(|window, cx| Composer::new("Comment", "Write", window, cx));
    let (events, _sub) = observe(&editor, cx);
    cx.update(|window, cx| {
        let input = editor.read(cx).input();
        input.update(cx, |i, cx| {
            i.replace_and_mark_text_in_range(None, "text", None, window, cx)
        });
        editor.update(cx, |e, cx| e.submit(&super::ComposerSubmit, window, cx));
    });
    assert!(
        events.borrow().is_empty(),
        "A marked composition is not sent"
    );
}

type Events = Rc<RefCell<Vec<ComposerEvent>>>;
fn observe(
    editor: &gpui_kit::Entity<Composer>,
    cx: &mut gpui_kit::VisualTestContext,
) -> (Events, gpui_kit::Subscription) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let seen = events.clone();
    let sub = cx.update(|_, cx| {
        cx.subscribe(editor, move |_, e: &ComposerEvent, _| {
            seen.borrow_mut().push(e.clone())
        })
    });
    (events, sub)
}

#[gpui_kit::test]
fn copied_files_reach_the_owner_without_pasting_local_paths_as_comment_text(
    cx: &mut TestAppContext,
) {
    use super::ComposerImagePaste;
    use gpui_kit::{ClipboardEntry, ClipboardItem, ExternalPaths};
    cx.update(gpui_kit::init);
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = Composer::new("New comment", "Leave a comment", window, cx);
        editor.set_images(true, cx);
        editor
    });
    let files = Rc::new(RefCell::new(0));
    let seen = files.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&editor, move |_, event: &ComposerImagePaste, _| {
            *seen.borrow_mut() += event.0.sources.len()
        })
    });
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| editor.focus(window, cx));
        window.render_frame(cx);
        cx.write_to_clipboard(ClipboardItem {
            entries: vec![ClipboardEntry::ExternalPaths(ExternalPaths(
                vec!["/fixture/Notes.txt".into()].into(),
            ))],
        });
        window.dispatch_action(Box::new(gpui_kit::component::input::Paste), cx);
    });
    cx.run_until_parked();
    assert_eq!(*files.borrow(), 1);
    cx.update(|_, cx| assert!(editor.read(cx).text(cx).is_empty()));
}
