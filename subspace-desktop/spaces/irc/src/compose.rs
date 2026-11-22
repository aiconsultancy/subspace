//! One retained composer per buffer: Enter sends the text (or `/command`)
//! to the runtime, typing notices are throttled, Up and Down recall sent
//! lines, a list above the input completes `@members`, `/commands`, their
//! arguments and `#channels`, and Tab completes a bare nick. A fleet
//! channel answers `/ask` itself, and a slim row under the input names the
//! keys.

use std::time::{Duration, Instant};

use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Styled as _,
    Subscription, Window,
};
use subspace_client_irc::{
    BufferKey, Command, MAX_INLINE_BYTES, Target, Typing, check_draft, connection_batches,
    draft_line_limit,
};
use subspace_framework::dashboard::chat_input::DraftLimits;
use subspace_framework::dashboard::chat_input::completion::CompletionList;
use subspace_framework::dashboard::{ChatInput, ChatInputEvent};

use crate::conversation::Conversation;

mod agents;
mod ask;
mod complete;
mod hints;
mod offer;
mod paste;
mod recall;
mod suggest;

pub(crate) use agents::{AgentNotice, listed_channel};
pub(crate) use recall::Recall;

/// While typing continues, an active notice is repeated at most this often.
const TYPING_REPEAT: Duration = Duration::from_secs(3);

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
            offer::accept(conversation, None, window, cx);
        }
        ChatInputEvent::Complete => complete::apply(conversation, window, cx),
        ChatInputEvent::Cancel => offer::close(conversation, cx),
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

/// Whether the conversation's connection sends multi-line text as batches,
/// which lets a draft hold more lines.
pub(crate) fn batches_lines(conversation: &Conversation, cx: &gpui_kit::App) -> bool {
    conversation
        .hub
        .read(cx)
        .state
        .network(&conversation.buffer.network)
        .is_some_and(|network| connection_batches(&network.caps))
}

/// Re-checks the draft against the connection's current limits, for caps
/// that arrive or change while a draft is open.
pub(crate) fn revalidate(conversation: &mut Conversation, cx: &mut Context<Conversation>) {
    let text = conversation.draft.entity.read(cx).text(cx);
    let batches = batches_lines(conversation, cx);
    validate_draft(&mut conversation.draft, &text, batches, cx);
}

/// Marks a draft over the limit for its connection: `batches` is whether
/// that connection sends multi-line text as one batch.
fn validate_draft(draft: &mut Draft, text: &str, batches: bool, cx: &mut Context<Conversation>) {
    let oversized = check_draft(text, batches).is_err();
    let limits = DraftLimits::new(draft_line_limit(text, batches), MAX_INLINE_BYTES);
    draft
        .entity
        .update(cx, |input, cx| input.set_limits(Some(limits), cx));
    if oversized != draft.oversized {
        draft.oversized = oversized;
        draft.entity.update(cx, |input, cx| {
            input.set_submit_blocked(oversized, cx);
        });
        cx.notify();
    }
}

fn submit(
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
    if refuse_agent(conversation, &text, cx) {
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

/// What to tell the channel after the draft changed, if anything.
pub(crate) fn typing_notice(text: &str, since: Option<Instant>, now: Instant) -> Option<Typing> {
    let command = text.trim_start().starts_with('/');
    match since {
        _ if command => since.map(|_| Typing::Done),
        None if !text.is_empty() => Some(Typing::Active),
        Some(_) if text.is_empty() => Some(Typing::Done),
        Some(since) if now.duration_since(since) >= TYPING_REPEAT => Some(Typing::Active),
        _ => None,
    }
}

fn typed(
    conversation: &mut Conversation,
    text: &str,
    now: Instant,
    cx: &mut Context<Conversation>,
) {
    let draft = &mut conversation.draft;
    let Some(state) = typing_notice(text, draft.typing_since, now) else {
        return;
    };
    draft.typing_since = (state == Typing::Active).then_some(now);
    conversation.send(
        Command::Typing {
            buffer: conversation.buffer.clone(),
            state,
        },
        cx,
    );
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use subspace_client_irc::{Target, Typing};

    use super::{placeholder, typing_notice};

    #[test]
    fn the_placeholder_names_where_the_message_goes() {
        assert_eq!(
            placeholder(&Target::Channel("#ops-deck".into())),
            "Message #ops-deck"
        );
        assert_eq!(placeholder(&Target::Query("mira".into())), "Message mira");
        assert_eq!(placeholder(&Target::Server), "Type a /command");
    }

    #[test]
    fn typing_is_announced_then_repeated_at_most_every_three_seconds() {
        let start = Instant::now();
        assert_eq!(typing_notice("h", None, start), Some(Typing::Active));
        let soon = start + Duration::from_secs(1);
        assert_eq!(typing_notice("he", Some(start), soon), None);
        let later = start + Duration::from_secs(3);
        assert_eq!(
            typing_notice("hel", Some(start), later),
            Some(Typing::Active)
        );
        assert_eq!(typing_notice("", Some(start), soon), Some(Typing::Done));
        assert_eq!(typing_notice("", None, soon), None);
    }

    #[test]
    fn commands_are_not_announced_as_typing() {
        let start = Instant::now();
        assert_eq!(typing_notice("/join", None, start), None);
        assert_eq!(
            typing_notice("/join", Some(start), start),
            Some(Typing::Done)
        );
    }
}
