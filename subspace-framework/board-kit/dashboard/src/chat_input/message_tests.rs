use super::ChatInput;
use crate::ChatInputEvent;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, Subscription, TestAppContext, VisualTestContext};
use std::{cell::RefCell, rc::Rc};

/// The texts `input` submits, recorded while the subscription lives.
pub(super) fn submissions(
    input: &Entity<ChatInput>,
    cx: &mut VisualTestContext,
) -> (Rc<RefCell<Vec<String>>>, Subscription) {
    let submitted = Rc::new(RefCell::new(Vec::new()));
    let recorded = submitted.clone();
    let subscription = cx.update(|_, cx| {
        cx.subscribe(input, move |_, event, _| {
            if let ChatInputEvent::Submit(text) = event {
                recorded.borrow_mut().push(text.clone());
            }
        })
    });
    (submitted, subscription)
}

#[gpui_kit::test]
fn message_enter_trims_rejects_empty_and_busy_and_error_preserves_draft(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (input, cx) = cx.add_window_view(ChatInput::message);
    let (submitted, _subscription) = submissions(&input, cx);
    cx.update(|window, cx| {
        input.update(cx, |input, cx| input.focus(window, cx));
        window.render_frame(cx);
        window.press("enter", cx);
    });
    cx.run_until_parked();
    assert!(submitted.borrow().is_empty());
    cx.update(|window, cx| {
        window.input("  retained message  ", cx);
        window.press("enter", cx);
    });
    cx.run_until_parked();
    assert_eq!(&*submitted.borrow(), &["retained message"]);
    cx.update(|window, cx| {
        input.update(cx, |input, cx| input.set_busy(true, cx));
        window.render_frame(cx);
        window.press("enter", cx);
        window.click("message-send", cx);
    });
    cx.run_until_parked();
    assert_eq!(submitted.borrow().len(), 1);
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.set_error("Fixture send failure".into(), cx);
            assert!(!input.disabled);
            assert!(!input.message.as_ref().unwrap().busy);
            assert_eq!(
                input.message.as_ref().unwrap().error.as_deref(),
                Some("Fixture send failure")
            );
            assert_eq!(input.text(cx), "  retained message  ");
            input.focus(window, cx);
        });
        window.render_frame(cx);
        window.press("enter", cx);
    });
    cx.run_until_parked();
    assert_eq!(
        &*submitted.borrow(),
        &["retained message", "retained message"]
    );
}

/// The Previous, Next and Complete events `input` emits, by name.
fn recall_events(
    input: &Entity<ChatInput>,
    cx: &mut VisualTestContext,
) -> (Rc<RefCell<Vec<&'static str>>>, Subscription) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let recorded = events.clone();
    let subscription = cx.update(|_, cx| {
        cx.subscribe(input, move |_, event, _| {
            let name = match event {
                ChatInputEvent::Previous => "previous",
                ChatInputEvent::Next => "next",
                ChatInputEvent::Complete => "complete",
                _ => return,
            };
            recorded.borrow_mut().push(name);
        })
    });
    (events, subscription)
}

#[gpui_kit::test]
fn plain_recall_input_hides_markdown_and_routes_arrows_and_tab(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(crate::bind_keys);
    let (input, cx) = cx.add_window_view(ChatInput::message);
    let (events, _subscription) = recall_events(&input, cx);
    cx.update(|window, cx| {
        input.update(cx, |input, cx| input.focus(window, cx));
        window.render_frame(cx);
        assert!(window.try_find("message-write").is_some());
        window.press("up", cx);
    });
    cx.run_until_parked();
    assert!(
        events.borrow().is_empty(),
        "arrows move the cursor by default"
    );
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.set_plain_text(cx);
            input.set_recall_keys(true, cx);
        });
        window.render_frame(cx);
        assert!(window.try_find("message-write").is_none());
        assert!(window.try_find("message-preview").is_none());
        window.press("up", cx);
        window.press("down", cx);
        window.press("tab", cx);
    });
    cx.run_until_parked();
    assert_eq!(&*events.borrow(), &["previous", "next", "complete"]);
}

#[gpui_kit::test]
fn multiline_draft_arrows_move_to_boundaries_until_history_is_active(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(crate::bind_keys);
    let (input, cx) = cx.add_window_view(ChatInput::message);
    let (events, _subscription) = recall_events(&input, cx);
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.set_plain_text(cx);
            input.set_recall_keys(true, cx);
            input.focus(window, cx);
            input.set_text("one\ntwo", window, cx);
        });
        window.render_frame(cx);
        window.press("up", cx);
        assert_eq!(input.read(cx).cursor(cx), 0);
        window.press("down", cx);
        assert_eq!(input.read(cx).cursor(cx), "one\ntwo".len());
        window.press("tab", cx);
    });
    cx.run_until_parked();
    assert_eq!(
        &*events.borrow(),
        &["complete"],
        "the arrows move the caret and Tab still completes"
    );
    cx.update(|window, cx| {
        input.update(cx, |input, cx| input.set_recalled(true, cx));
        window.render_frame(cx);
        window.press("up", cx);
    });
    cx.run_until_parked();
    assert_eq!(
        events.borrow().last(),
        Some(&"previous"),
        "a recalled multi-line line keeps recalling"
    );
}

#[gpui_kit::test]
fn shift_enter_breaks_the_line_without_sending(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(crate::bind_keys);
    let (input, cx) = cx.add_window_view(ChatInput::message);
    let (submitted, _subscription) = submissions(&input, cx);
    cx.update(|window, cx| {
        input.update(cx, |input, cx| input.focus(window, cx));
        window.render_frame(cx);
        window.input("first", cx);
        window.press("shift-enter", cx);
        window.input("second", cx);
    });
    cx.run_until_parked();
    assert!(
        submitted.borrow().is_empty(),
        "Shift+Enter only breaks the line"
    );
    assert_eq!(cx.update(|_, cx| input.read(cx).text(cx)), "first\nsecond");
    cx.update(|window, cx| window.press("enter", cx));
    cx.run_until_parked();
    assert_eq!(&*submitted.borrow(), &["first\nsecond"]);
}

#[gpui_kit::test]
fn clipboard_attachments_obey_the_owners_encoded_byte_limit(cx: &mut TestAppContext) {
    use super::super::attachments::AttachmentBudget;
    use gpui_kit::{ClipboardEntry, ClipboardItem, Image, ImageFormat};
    cx.update(gpui_kit::init);
    let (input, cx) = cx.add_window_view(ChatInput::message);
    let received = Rc::new(RefCell::new(Vec::new()));
    let recorded = received.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&input, move |_, event, _| {
            if let ChatInputEvent::PasteAttachments(batch) = event {
                recorded.borrow_mut().push(batch.batch().clone());
            }
        })
    });
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.set_attachment_budget(
                AttachmentBudget::new(1, 12_000_000).with_image_limit(12_000_000),
                cx,
            );
            input.focus(window, cx);
        });
        window.render_frame(cx);
        cx.write_to_clipboard(ClipboardItem {
            entries: vec![ClipboardEntry::Image(Image::from_bytes(
                ImageFormat::Png,
                vec![0; 12_000_001],
            ))],
        });
        window.press(crate::secondary!("v"), cx);
    });
    cx.run_until_parked();
    assert_eq!(received.borrow().len(), 1);
    assert!(received.borrow()[0].sources.is_empty());
    assert!(received.borrow()[0].notice.is_some());
}

#[gpui_kit::test]
fn an_open_list_says_whether_tab_or_enter_completed(cx: &mut TestAppContext) {
    use crate::chat_input::CompletionKey;
    cx.update(gpui_kit::init);
    cx.update(crate::bind_keys);
    let (input, cx) = cx.add_window_view(ChatInput::message);
    let keys = Rc::new(RefCell::new(Vec::new()));
    let recorded = keys.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&input, move |input, event, cx| {
            if let ChatInputEvent::Complete = event {
                recorded.borrow_mut().push(input.read(cx).completion_key());
            }
        })
    });
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.set_completing(true, cx);
        });
        window.render_frame(cx);
        window.press("tab", cx);
    });
    cx.run_until_parked();
    // Each key is its own dispatch, so the owner reads its own key.
    cx.update(|window, cx| window.press("enter", cx));
    cx.run_until_parked();
    assert_eq!(
        &*keys.borrow(),
        &[Some(CompletionKey::Tab), Some(CompletionKey::Enter)]
    );
}
