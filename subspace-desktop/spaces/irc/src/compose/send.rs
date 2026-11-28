//! Sending a draft: refusing a private message to a fleet agent, `/ask` in
//! a fleet channel, and every other line or command, then emptying the
//! composer.

use gpui_kit::{Context, Window};
use subspace_client_irc::Command;

use super::{ask, batches_lines, validate_draft};
use crate::conversation::Conversation;

pub(super) fn submit(
    conversation: &mut Conversation,
    text: String,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    let batches = batches_lines(conversation, cx);
    validate_draft(&mut conversation.draft, &text, batches, cx);
    if conversation.draft.oversized {
        return;
    }
    if text.is_empty() && conversation.tray.is_empty() {
        return;
    }
    // Before the attachment sender: an upload would message the agent too.
    if super::gates::refused(conversation, &text, cx) {
        return;
    }
    if crate::tracker::submit(conversation, &text, window, cx) {
        return;
    }
    if crate::uploads::outgoing::submit(conversation, text.clone(), window, cx) {
        conversation.draft.typing_since = None;
        return;
    }
    if text.is_empty() {
        return;
    }
    if let Some(asked) = ask::ask(&text, &conversation.buffer, &conversation.roster) {
        let asked = ask::within_limits(asked, batches);
        send_ask(conversation, asked, text, window, cx);
        return;
    }
    let command = Command::Input {
        buffer: conversation.buffer.clone(),
        text: text.clone(),
    };
    sent(conversation, command, text, window, cx);
}

/// Sends `/ask` as its message, or keeps the draft and says why not.
fn send_ask(
    conversation: &mut Conversation,
    asked: ask::Asked,
    text: String,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    match asked {
        ask::Asked::Send(command) => sent(conversation, command, text, window, cx),
        ask::Asked::Refused(reason) => conversation
            .draft
            .entity
            .update(cx, |input, cx| input.set_error(reason.to_owned(), cx)),
    }
}

/// Sends `command` for draft `text`, which joins the recalled lines, and
/// empties the composer.
fn sent(
    conversation: &mut Conversation,
    command: Command,
    text: String,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    conversation.send(command, cx);
    let draft = &mut conversation.draft;
    draft.typing_since = None;
    draft.recall.push(text);
    draft.entity.update(cx, |input, cx| {
        // Sent: an earlier refusal, such as an empty `/ask`, no longer holds.
        input.set_busy(false, cx);
        input.clear(window, cx);
    });
}
