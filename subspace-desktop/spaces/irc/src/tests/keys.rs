//! The composer's Return, Shift+Return and arrow keys in the real board.

use gpui_kit::component::input::Paste;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{ClipboardItem, Entity, TestAppContext, VisualTestContext};
use subspace_client_irc::Command;

use super::completion::{composer, labels, text};
use crate::conversation::Conversation;
use crate::fixtures::channel;

/// The text of every `Command::Input` the conversation sent.
fn sent(conversation: &Entity<Conversation>, cx: &mut VisualTestContext) -> Vec<String> {
    let hub = conversation.read_with(cx, |conversation, _| conversation.hub.clone());
    hub.read_with(cx, |hub, _| {
        hub.sent()
            .iter()
            .filter_map(|command| match command {
                Command::Input { buffer, text } if *buffer == channel("#ship-galactica") => {
                    Some(text.clone())
                }
                _ => None,
            })
            .collect()
    })
}

#[gpui_kit::test]
fn an_oversized_paste_stays_editable_and_saves_the_exact_draft(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("paste.md");
    let markdown = format!("```md\n\n{}```", "  Café\n".repeat(597));
    assert_eq!(markdown.lines().count(), 600);
    let (conversation, input, cx) = composer(cx);
    cx.update(|window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(markdown.clone()));
        window.dispatch_action(Box::new(Paste), cx);
    });
    cx.run_until_parked();
    let draft = text(&input, cx);
    assert!(draft == markdown, "all clipboard text is present");
    cx.simulate_keystrokes("enter");
    assert!(sent(&conversation, cx).is_empty());
    assert!(text(&input, cx) == draft, "the paste is unchanged");

    cx.update(|window, cx| window.click("irc-save-draft", cx));
    cx.run_until_parked();
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    assert!(text(&input, cx) == draft, "the paste is unchanged");

    cx.update(|window, cx| window.click("irc-save-draft", cx));
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| Some(path.clone()));
    cx.run_until_parked();
    assert_eq!(std::fs::read(path).unwrap(), draft.as_bytes());
    assert!(sent(&conversation, cx).is_empty());

    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_range(0..draft.len(), "short reply", window, cx);
            input.focus(window, cx);
        });
    });
    cx.simulate_keystrokes("enter");
    assert_eq!(sent(&conversation, cx), ["short reply"]);
}

#[gpui_kit::test]
fn recall_rechecks_the_limit_when_it_replaces_or_restores_an_oversized_draft(
    cx: &mut TestAppContext,
) {
    let (conversation, input, cx) = composer(cx);
    cx.simulate_input("earlier");
    cx.simulate_keystrokes("enter");
    let oversized = "é".repeat(2049);
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_range(0..0, &oversized, window, cx);
        });
    });
    cx.simulate_keystrokes("up");
    assert_eq!(text(&input, cx), "earlier");
    cx.simulate_keystrokes("down enter");
    assert!(
        text(&input, cx) == oversized,
        "the oversized draft is restored"
    );
    assert_eq!(sent(&conversation, cx), ["earlier"]);
    cx.simulate_keystrokes("up enter");
    assert_eq!(sent(&conversation, cx), ["earlier", "earlier"]);
}

#[gpui_kit::test]
fn shift_return_breaks_the_line_and_return_sends_both_lines(cx: &mut TestAppContext) {
    let (conversation, input, cx) = composer(cx);
    cx.simulate_input("first line");
    cx.simulate_keystrokes("shift-enter");
    cx.simulate_input("second line");
    assert_eq!(text(&input, cx), "first line\nsecond line");
    assert!(sent(&conversation, cx).is_empty(), "nothing sent yet");

    cx.simulate_keystrokes("enter");
    assert_eq!(sent(&conversation, cx), ["first line\nsecond line"]);
    assert_eq!(text(&input, cx), "", "the draft clears");
}

#[gpui_kit::test]
fn shift_return_with_the_list_open_breaks_the_line_and_closes_it(cx: &mut TestAppContext) {
    let (conversation, input, cx) = composer(cx);
    cx.simulate_input("@gal");
    assert!(!labels(&conversation, cx).is_empty(), "the list is open");
    cx.simulate_keystrokes("shift-enter");
    assert_eq!(text(&input, cx), "@gal\n");
    assert!(labels(&conversation, cx).is_empty(), "and closes");
    assert!(sent(&conversation, cx).is_empty());
}

#[gpui_kit::test]
fn return_sends_after_the_completion_list_closes(cx: &mut TestAppContext) {
    let (conversation, input, cx) = composer(cx);
    cx.simulate_input("ask @gal");
    cx.simulate_keystrokes("escape");
    cx.simulate_input(" now");
    cx.simulate_keystrokes("enter");
    assert_eq!(sent(&conversation, cx), ["ask @gal now"]);
    assert_eq!(text(&input, cx), "");
}

#[gpui_kit::test]
fn up_recalls_a_sent_line_only_from_a_single_line_draft(cx: &mut TestAppContext) {
    let (_, input, cx) = composer(cx);
    cx.simulate_input("earlier");
    cx.simulate_keystrokes("enter");

    cx.simulate_input("one");
    cx.simulate_keystrokes("shift-enter");
    cx.simulate_input("two");
    cx.simulate_keystrokes("up");
    assert_eq!(
        text(&input, cx),
        "one\ntwo",
        "Up moves between the draft's lines"
    );
    let cursor = input.read_with(cx, |input, cx| input.cursor(cx));
    assert!(cursor <= "one".len(), "the cursor is on the first line");
    cx.simulate_keystrokes("down");
    assert_eq!(text(&input, cx), "one\ntwo", "Down too");
    let cursor = input.read_with(cx, |input, cx| input.cursor(cx));
    assert!(
        cursor > "one".len(),
        "the cursor is back on the second line"
    );

    cx.update(|window, cx| {
        input.update(cx, |input, cx| input.set_text("one", window, cx));
        window.render_frame(cx);
    });
    cx.simulate_keystrokes("up");
    assert_eq!(text(&input, cx), "earlier", "a single line recalls");
    cx.simulate_keystrokes("down");
    assert_eq!(text(&input, cx), "one", "and Down returns to the draft");
}

#[gpui_kit::test]
fn a_recalled_multi_line_message_does_not_stop_the_recall(cx: &mut TestAppContext) {
    let (_, input, cx) = composer(cx);
    cx.simulate_input("earlier");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("first");
    cx.simulate_keystrokes("shift-enter");
    cx.simulate_input("second");
    cx.simulate_keystrokes("enter");

    cx.simulate_input("draft");
    cx.simulate_keystrokes("up");
    assert_eq!(text(&input, cx), "first\nsecond");
    cx.simulate_keystrokes("up");
    assert_eq!(text(&input, cx), "earlier", "Up goes on past it");
    cx.simulate_keystrokes("down down");
    assert_eq!(text(&input, cx), "draft", "and Down comes back");

    cx.simulate_keystrokes("up");
    cx.simulate_input("!");
    let edited = text(&input, cx);
    assert_ne!(edited, "first\nsecond");
    cx.simulate_keystrokes("up down");
    assert_eq!(
        text(&input, cx),
        edited,
        "once edited, it is a draft and the arrows move the cursor"
    );
}
