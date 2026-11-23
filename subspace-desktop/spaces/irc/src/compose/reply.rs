//! Reply mode: the composer writes into a thread, chosen with **Reply in
//! thread** in the inspector's Thread tab. The message goes to the channel
//! like any other, so everyone there sees it, with the server's reply tag
//! (`+draft/reply`) naming the thread's root
//! ([`subspace_client_irc::Command::Reply`]). Nothing here sends a private
//! message: reply mode exists only in a channel, which an owner rule keeps
//! as the one place anything is said to an agent.
//!
//! A row above the input says it is a reply and to what, with a way to
//! cancel; Escape cancels too. The composer leaves reply mode once the
//! reply is sent. When the thread's root is no longer held, as under the
//! line cap, the draft stays in reply mode, marked gone: the row says the
//! thread is no longer loaded, Enter sends nothing, and only an explicit
//! **Write to the channel** leaves reply mode, keeping the text, so a
//! reply never silently becomes an ordinary message or a command.

use gpui_kit::component::{Icon, Sizable as _, h_flex};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
};
use subspace_client_irc::{BufferKey, Command, Target};
use subspace_framework::dashboard::action::{Intent, icon_button};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, truncated};

use crate::conversation::Conversation;
use crate::state::Network;

/// The most characters of the root's text the reply row quotes.
const EXCERPT_CHARS: usize = 80;

/// The thread a draft replies in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReplyTo {
    /// The root message's ID, which the reply tag names.
    pub root: String,
    /// Who wrote the root.
    pub author: String,
    /// The start of the root's text, plain.
    pub excerpt: String,
    /// The root is no longer held: nothing is sent until the person
    /// chooses to write to the channel instead.
    pub gone: bool,
}

impl ReplyTo {
    /// The thread of `root`, with message ID `id`, written by `author`.
    pub(crate) fn new(id: &str, author: &str, text: &str) -> Self {
        let plain = subspace_client_irc::format::strip(text);
        let mut chars = plain.split_whitespace().collect::<Vec<_>>().join(" ");
        if chars.chars().count() > EXCERPT_CHARS {
            chars = chars.chars().take(EXCERPT_CHARS - 1).collect();
            chars.push('…');
        }
        Self {
            root: id.to_owned(),
            author: author.to_owned(),
            excerpt: chars,
            gone: false,
        }
    }

    /// What the row above the input says.
    pub(crate) fn words(&self) -> String {
        format!("Replying in thread to {}: {}", self.author, self.excerpt)
    }
}

/// What the composer says in reply mode while empty.
pub(crate) fn placeholder(reply: &ReplyTo) -> String {
    format!("Reply in thread to {}", reply.author)
}

/// Why a draft refused in reply mode was kept.
pub(crate) const COMMAND: &str =
    "A thread reply is sent as text. Cancel the reply to run a command.";
/// Why a draft with attachments in reply mode was kept.
pub(crate) const ATTACHMENTS: &str =
    "Attachments can't go in a thread reply yet. Cancel the reply to send them.";
/// Why a reply whose thread is no longer loaded was kept.
pub(crate) const GONE: &str = "The thread is no longer loaded, so this reply was not sent. Choose Write to the channel to send it as an ordinary message.";
/// Why reply mode is refused outside a channel.
pub(crate) const CHANNEL_ONLY: &str = "Thread replies are sent in a channel.";

/// The command that sends `text` as a reply in `reply`'s thread to
/// `buffer`'s channel, or why it cannot be sent. A leading `//` is a
/// literal slash, as in the composer; any other `/command` is refused.
pub(crate) fn command(
    buffer: &BufferKey,
    reply: &ReplyTo,
    text: &str,
    network: Option<&Network>,
) -> Result<Command, &'static str> {
    if !matches!(buffer.target, Target::Channel(_)) {
        return Err(CHANNEL_ONLY);
    }
    if reply.gone {
        return Err(GONE);
    }
    if let Some(refusal) = crate::thread::reply_refusal(network) {
        return Err(refusal);
    }
    let text = match text.strip_prefix('/') {
        Some(rest) if rest.starts_with('/') => rest,
        Some(_) => return Err(COMMAND),
        None => text,
    };
    Ok(Command::Reply {
        buffer: buffer.clone(),
        msgid: reply.root.clone(),
        text: text.to_owned(),
    })
}

/// Puts the composer into reply mode for `reply` and gives it the
/// keyboard. Only in a channel.
pub(crate) fn start(
    conversation: &mut Conversation,
    reply: ReplyTo,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    if !matches!(conversation.buffer.target, Target::Channel(_)) {
        return;
    }
    let hint = placeholder(&reply);
    conversation.draft.reply = Some(reply);
    conversation.draft.entity.update(cx, |input, cx| {
        input.set_placeholder(hint, window, cx);
        input.focus(window, cx);
    });
    cx.notify();
}

/// Leaves reply mode, if the composer is in it.
pub(crate) fn cancel(
    conversation: &mut Conversation,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    if conversation.draft.reply.take().is_none() {
        return;
    }
    let hint = super::placeholder(&conversation.buffer.target);
    conversation
        .draft
        .entity
        .update(cx, |input, cx| input.set_placeholder(hint, window, cx));
    cx.notify();
}

/// Marks the reply to `root`, whose thread is no longer held, as gone: it
/// keeps the draft in reply mode and sends nothing until the person
/// chooses to write to the channel instead.
pub(crate) fn forget(conversation: &mut Conversation, root: &str, cx: &mut Context<Conversation>) {
    if let Some(reply) = conversation
        .draft
        .reply
        .as_mut()
        .filter(|reply| reply.root == root)
    {
        reply.gone = true;
        cx.notify();
    }
}

/// The row above the input in reply mode: what is replied to, and Cancel.
pub(super) fn row(reply: &ReplyTo, cx: &mut Context<Conversation>) -> gpui_kit::AnyElement {
    if reply.gone {
        return gone_row(cx).into_any_element();
    }
    let tokens = Tokens::current(cx);
    let words = reply.words();
    h_flex()
        .id("irc-reply-mode")
        .test_support()
        .role(Role::Status)
        .aria_label(SharedString::from(words.clone()))
        .w_full()
        .min_w_0()
        .gap_2()
        .px_2()
        .py_1()
        .text_size(tokens.secondary_size())
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .text_color(tokens.muted)
                .child(Icon::from(IconName::Reply).xsmall()),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_color(tokens.muted)
                .child("Replying in thread to"),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_color(tokens.fg_strong)
                .child(format!("{}:", reply.author)),
        )
        .child(
            truncated::text("irc-reply-mode-excerpt", reply.excerpt.clone())
                .flex_1()
                .text_color(tokens.fg),
        )
        .child(
            icon_button(
                "irc-reply-mode-cancel",
                Intent::Quiet,
                IconName::X,
                "Cancel reply",
            )
            .on_click(cx.listener(|this, _, window, cx| cancel(this, window, cx))),
        )
        .into_any_element()
}

/// The row once the thread is no longer loaded: why nothing was sent, and
/// the one way on, which keeps the text as an ordinary draft.
fn gone_row(cx: &mut Context<Conversation>) -> impl IntoElement {
    let tokens = Tokens::current(cx);
    h_flex()
        .id("irc-reply-mode")
        .test_support()
        .role(Role::Alert)
        .aria_label(SharedString::from(GONE))
        .w_full()
        .min_w_0()
        .gap_2()
        .px_2()
        .py_1()
        .text_size(tokens.secondary_size())
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_color(tokens.fg)
                .child("The thread is no longer loaded. Your reply was not sent."),
        )
        .child(
            subspace_framework::dashboard::action::action_button(
                "irc-reply-mode-to-channel",
                Intent::Neutral,
                Some(IconName::Hash),
                "Write to the channel",
            )
            .on_click(cx.listener(|this, _, window, cx| cancel(this, window, cx))),
        )
}

#[cfg(test)]
mod tests;
