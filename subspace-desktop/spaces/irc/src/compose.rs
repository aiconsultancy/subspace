//! One retained composer per buffer: Enter sends the text (or `/command`)
//! to the runtime, typing notices are throttled, Up and Down recall sent
//! lines, a list above the input completes `@members`, `/commands`, their
//! arguments and `#channels`, and Tab completes a bare nick. A fleet
//! channel answers `/ask` itself, and a slim row under the input names the
//! keys.

use std::time::Instant;

use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Styled as _,
    Subscription, Window,
};
use subspace_client_irc::{BufferKey, MAX_INLINE_BYTES, Target, draft_line_limit};
use subspace_framework::dashboard::chat_input::completion::CompletionList;
use subspace_framework::dashboard::chat_input::{CompletionKey, DraftLimits};
use subspace_framework::dashboard::{ChatInput, ChatInputEvent};

use crate::conversation::Conversation;

mod agents;
mod ask;
mod complete;
mod hints;
mod limits;
mod offer;
#[cfg(test)]
pub(crate) use offer::RUN_CHANGED;
mod paste;
mod recall;
pub(crate) mod reply;
mod send;
pub(crate) mod suggest;
mod typing;

pub(crate) use agents::{AgentNotice, listed_channel};
use limits::validate_draft;
pub(crate) use limits::{batches_lines, revalidate};
pub(crate) use recall::Recall;
use send::submit;
use typing::typed;

/// One conversation's composer: its input, typing notices and sent lines.
pub(crate) struct Draft {
    pub entity: Entity<ChatInput>,
    /// When we last told the channel we are typing; `None` when idle.
    typing_since: Option<Instant>,
    pub recall: Recall,
    oversized: bool,
    saving: bool,
    /// The choices for the word at the cursor, above the input.
    list: Entity<CompletionList>,
    /// What the list offers now; `None` while it is closed.
    offer: Option<suggest::Offer>,
    /// Why a private message to a fleet agent was not sent or opened.
    pub agent_notice: Option<AgentNotice>,
    /// The thread the draft replies in, in reply mode.
    pub reply: Option<reply::ReplyTo>,
    _subscription: Subscription,
}

impl Draft {
    /// A plain-text composer for `buffer` that reports to its conversation.
    pub fn new(buffer: &BufferKey, window: &mut Window, cx: &mut Context<Conversation>) -> Self {
        let hint = placeholder(&buffer.target);
        let entity = cx.new(|cx| {
            let mut input = ChatInput::message(window, cx);
            input.set_plain_text(cx);
            input.set_attachment_budget(crate::uploads::outgoing::Tray::default().budget(), cx);
            input.set_recall_keys(true, cx);
            input.set_placeholder(hint, window, cx);
            input.set_limits(
                Some(DraftLimits::new(
                    draft_line_limit("", false),
                    MAX_INLINE_BYTES,
                )),
                cx,
            );
            input
        });
        let list = cx.new(|cx| CompletionList::new("irc-completion", window, cx));
        let subscription = cx.subscribe_in(&list, window, offer::clicked);
        Self {
            entity,
            typing_since: None,
            recall: Recall::default(),
            oversized: false,
            saving: false,
            list,
            offer: None,
            agent_notice: None,
            reply: None,
            _subscription: subscription,
        }
    }

    /// The labels the completion list shows.
    #[cfg(test)]
    pub fn offered(&self, cx: &gpui_kit::App) -> Vec<String> {
        let list = self.list.read(cx);
        list.items()
            .iter()
            .map(|item| item.label().to_string())
            .collect()
    }

    /// The feedback, completions and editor for this draft.
    pub fn render(
        &self,
        tray: &crate::uploads::outgoing::Tray,
        uploads: bool,
        cx: &mut Context<Conversation>,
    ) -> AnyElement {
        v_flex()
            .w_full()
            .children(
                self.agent_notice
                    .as_ref()
                    .map(|notice| agents::notice(notice, cx)),
            )
            .children(self.oversized.then(|| paste::notice(self.saving, cx)))
            .children(self.reply.as_ref().map(|replying| reply::row(replying, cx)))
            .children(uploads.then(|| crate::uploads::outgoing::controls(&self.entity, tray, cx)))
            .child(self.list.clone())
            .child(self.entity.clone())
            .child(hints::row(uploads, cx))
            .into_any_element()
    }
}

/// What the empty composer says: whom the message goes to.
pub(crate) fn placeholder(target: &Target) -> String {
    match target {
        Target::Channel(name) | Target::Query(name) => format!("Message {name}"),
        Target::Server => "Type a /command".to_owned(),
    }
}

pub(crate) fn handle(
    conversation: &mut Conversation,
    event: &ChatInputEvent,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    match event {
        ChatInputEvent::Submit(text) => submit(conversation, text.clone(), window, cx),
        ChatInputEvent::PasteAttachments(paste) => {
            crate::uploads::outgoing::ingest(conversation, paste.batch().clone(), cx)
        }
        ChatInputEvent::Changed(text) => changed(conversation, text, cx),
        ChatInputEvent::Previous if offer::is_open(conversation) => {
            offer::step(conversation, -1, cx)
        }
        ChatInputEvent::Next if offer::is_open(conversation) => offer::step(conversation, 1, cx),
        ChatInputEvent::Previous => recall::step(conversation, true, window, cx),
        ChatInputEvent::Next => recall::step(conversation, false, window, cx),
        ChatInputEvent::Complete if offer::is_open(conversation) => {
            let key = conversation.draft.entity.read(cx).completion_key();
            let key = key.unwrap_or(CompletionKey::Tab);
            offer::accept(conversation, offer::Pick::Key(key), window, cx);
        }
        ChatInputEvent::Complete => complete::apply(conversation, window, cx),
        ChatInputEvent::Cancel if offer::is_open(conversation) => offer::close(conversation, cx),
        ChatInputEvent::Cancel => reply::escape(conversation, window, cx),
    }
}

fn changed(conversation: &mut Conversation, text: &str, cx: &mut Context<Conversation>) {
    let batches = batches_lines(conversation, cx);
    validate_draft(&mut conversation.draft, text, batches, cx);
    recall::follow(conversation, text, cx);
    if conversation.draft.oversized {
        typed(conversation, "", Instant::now(), cx);
        offer::close(conversation, cx);
    } else {
        typed(conversation, text, Instant::now(), cx);
        offer::refresh(conversation, cx);
    }
}

#[cfg(test)]
mod tests {
    use subspace_client_irc::Target;

    use super::placeholder;

    #[test]
    fn the_placeholder_names_where_the_message_goes() {
        assert_eq!(
            placeholder(&Target::Channel("#ops-deck".into())),
            "Message #ops-deck"
        );
        assert_eq!(placeholder(&Target::Query("mira".into())), "Message mira");
        assert_eq!(placeholder(&Target::Server), "Type a /command");
    }
}
