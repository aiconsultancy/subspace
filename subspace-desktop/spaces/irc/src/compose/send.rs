//! Sending a draft: refusing a private message to a fleet agent, `/ask` in
//! a fleet channel, and every other line or command, then emptying the
//! composer.

use gpui_kit::{Context, Window};
use subspace_client_irc::{Command, Target};

use super::{AgentNotice, agents, ask, batches_lines, reply, validate_draft};
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
    if refused_privately(conversation, &text, cx) {
        return;
    }
    if let Some(replying) = conversation.draft.reply.clone() {
        send_reply(conversation, &replying, text, window, cx);
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

/// Sends `text` as a reply in `replying`'s thread and leaves reply mode,
/// or keeps the draft and says why not.
fn send_reply(
    conversation: &mut Conversation,
    replying: &reply::ReplyTo,
    text: String,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    let refusal = if conversation.tray.is_empty() {
        let state = &conversation.hub.read(cx).state;
        let network = state.network(&conversation.buffer.network);
        reply::command(&conversation.buffer, replying, &text, network)
    } else {
        Err(reply::ATTACHMENTS)
    };
    match refusal {
        Ok(command) => {
            sent(conversation, command, text, window, cx);
            reply::cancel(conversation, window, cx);
        }
        Err(reason) => conversation
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

/// Keeps a draft that would message a fleet agent privately, or send a
/// private TAGMSG the reactions gate refuses.
fn refused_privately(
    conversation: &mut Conversation,
    text: &str,
    cx: &mut Context<Conversation>,
) -> bool {
    refuse_agent(conversation, text, cx) || refuse_tag(conversation, text, cx)
}

/// Keeps a draft whose hand-typed TAGMSG, which can carry a reaction,
/// goes to someone privately whom the reactions gate refuses
/// ([`crate::reactions::gate::refusal`]): a fleet agent, or anyone while
/// the initiatives are not known.
fn refuse_tag(conversation: &mut Conversation, text: &str, cx: &mut Context<Conversation>) -> bool {
    let refusal = subspace_client_irc::private_tag_recipients(text)
        .into_iter()
        .find_map(|nick| {
            crate::reactions::gate::refusal(&Target::Query(nick), &conversation.roster)
        });
    let refused = refusal.is_some();
    if let Some(words) = refusal {
        conversation
            .draft
            .entity
            .update(cx, |input, cx| input.set_error(words, cx));
    }
    refused
}

/// Keeps a draft that would message a fleet agent privately, and shows why.
/// True when it was kept.
fn refuse_agent(
    conversation: &mut Conversation,
    text: &str,
    cx: &mut Context<Conversation>,
) -> bool {
    let nicks = agents::direct_targets(text, &conversation.buffer.target);
    let state = &conversation.hub.read(cx).state;
    let network = &conversation.buffer.network;
    let notice = AgentNotice::refusing_any(&conversation.roster, &nicks, |channel| {
        agents::listed_channel(state, network, channel).is_some()
    });
    let refused = notice.is_some();
    if refused {
        conversation.draft.agent_notice = notice;
        cx.notify();
    }
    refused
}
