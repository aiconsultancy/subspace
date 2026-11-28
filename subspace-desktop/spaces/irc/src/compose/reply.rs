//! One retained inspector composer per IRC message ID, independent of the
//! channel editor. Submitting targets that message with the server's reply tag.

use std::collections::{HashMap, HashSet};

use gpui_kit::component::v_flex;
use gpui_kit::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, TestSupportExt as _, Window,
};
use subspace_client_irc::{
    BufferKey, Command, MAX_INLINE_BYTES, Target, check_draft, connection_batches, draft_line_limit,
};
use subspace_framework::dashboard::chat_input::DraftLimits;
use subspace_framework::dashboard::chat_input::completion::{CompletionEvent, CompletionList};
use subspace_framework::dashboard::{ChatInput, ChatInputEvent};

use super::recall::{self, Recall};
use super::suggest::{Facts, Offer, suggest};
use crate::conversation::Conversation;
use crate::hub::Hub;
use crate::state::{IrcState, Network};

#[cfg(test)]
mod tests;

pub(crate) const COMMAND: &str =
    "Use the channel composer for commands. Start with // for a literal slash.";
pub(crate) const ATTACHMENTS: &str = "Use the channel composer to attach files.";
pub(crate) const GONE: &str = "The original message is no longer loaded. Draft kept.";
const TOO_LONG: &str = "This reply is too long to send in IRC.";

#[derive(Default)]
pub(crate) struct Drafts {
    by_root: HashMap<String, Entity<ReplyDraft>>,
    selected: Option<Selected>,
}

struct Selected {
    root: String,
    seq: i64,
    _observe: Subscription,
}

impl Drafts {
    pub(crate) fn get(&self, root: &str) -> Option<Entity<ReplyDraft>> {
        self.by_root.get(root).cloned()
    }

    pub(crate) fn remove(&mut self, root: &str) {
        if self
            .selected
            .as_ref()
            .is_some_and(|selected| selected.root == root)
        {
            self.selected = None;
        }
        self.by_root.remove(root);
    }

    pub(crate) fn selected_seq(&self) -> Option<i64> {
        self.selected.as_ref().map(|selected| selected.seq)
    }

    pub(crate) fn selected_with_text(&self, cx: &App) -> Option<&str> {
        let selected = self.selected.as_ref()?;
        self.by_root
            .get(&selected.root)?
            .read(cx)
            .has_text(cx)
            .then_some(selected.root.as_str())
    }

    pub(crate) fn missing(
        &self,
        buffer: &BufferKey,
        state: &IrcState,
        cx: &App,
    ) -> Vec<Entity<ReplyDraft>> {
        let written: Vec<_> = self
            .by_root
            .iter()
            .filter(|(_, draft)| draft.read(cx).has_text(cx))
            .collect();
        if written.is_empty() {
            return Vec::new();
        }
        let loaded = loaded_ids(buffer, state);
        let mut missing: Vec<_> = written
            .into_iter()
            .filter(|(root, _)| !loaded.contains(root.as_str()))
            .collect();
        missing.sort_unstable_by_key(|(root, _)| *root);
        missing
            .into_iter()
            .map(|(_, draft)| draft.clone())
            .collect()
    }
}

fn loaded_ids<'a>(buffer: &BufferKey, state: &'a IrcState) -> HashSet<&'a str> {
    state
        .buffer(buffer)
        .into_iter()
        .flat_map(|held| held.entries.iter())
        .filter_map(|entry| entry.line.id.as_deref())
        .collect()
}

pub(crate) fn select(
    conversation: &mut Conversation,
    target: (&str, i64),
    window: &mut Window,
    cx: &mut Context<Conversation>,
) -> Entity<ReplyDraft> {
    let (root, seq) = target;
    let draft = conversation.reply_drafts.get(root).unwrap_or_else(|| {
        let buffer = conversation.buffer.clone();
        let hub = conversation.hub.clone();
        cx.new(|cx| ReplyDraft::new(buffer, root.to_owned(), hub, window, cx))
    });
    let previous = conversation.reply_drafts.selected.replace(Selected {
        root: root.to_owned(),
        seq,
        _observe: cx.observe(&draft, |_, _, cx| cx.notify()),
    });
    conversation
        .reply_drafts
        .by_root
        .insert(root.to_owned(), draft.clone());
    if let Some(previous) = previous
        && previous.root != root
        && conversation
            .reply_drafts
            .by_root
            .get(&previous.root)
            .is_some_and(|old| !old.read(cx).keep(cx))
    {
        conversation.reply_drafts.by_root.remove(&previous.root);
    }
    cx.notify();
    draft
}

/// The editor and completion state for one reply. Its destination never changes.
pub(crate) struct ReplyDraft {
    root: String,
    buffer: BufferKey,
    hub: Entity<Hub>,
    input: Entity<ChatInput>,
    list: Entity<CompletionList>,
    offer: Option<Offer>,
    recall: Recall,
    _subscriptions: [Subscription; 3],
}

impl ReplyDraft {
    fn new(
        buffer: BufferKey,
        root: String,
        hub: Entity<Hub>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            let mut input = ChatInput::compact_message("Reply", window, cx);
            input.set_recall_keys(true, cx);
            input.set_placeholder("Write a reply…", window, cx);
            input
        });
        super::tools::install(&input, None, cx);
        let list = cx.new(|cx| CompletionList::new("irc-reply-completion", window, cx));
        let subscriptions = [
            cx.subscribe_in(&input, window, |this, _, event, window, cx| {
                this.handle(event, window, cx)
            }),
            cx.subscribe_in(&list, window, |this, _, event, window, cx| {
                let CompletionEvent::Accept(index) = *event;
                this.accept(index, window, cx);
            }),
            cx.observe(&hub, |this, _, cx| this.revalidate(cx)),
        ];
        let mut draft = Self {
            root,
            buffer,
            hub,
            input,
            list,
            offer: None,
            recall: Recall::default(),
            _subscriptions: subscriptions,
        };
        draft.revalidate(cx);
        draft
    }

    pub(crate) fn root(&self) -> &str {
        &self.root
    }

    pub(crate) fn text(&self, cx: &App) -> String {
        self.input.read(cx).text(cx)
    }

    pub(crate) fn has_text(&self, cx: &App) -> bool {
        !self.text(cx).is_empty()
    }

    fn keep(&self, cx: &App) -> bool {
        self.has_text(cx) || !self.recall.is_empty()
    }

    pub(crate) fn focus(&self, window: &mut Window, cx: &mut App) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    #[cfg(test)]
    pub(crate) fn input(&self) -> &Entity<ChatInput> {
        &self.input
    }

    fn handle(&mut self, event: &ChatInputEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            ChatInputEvent::Submit(text) => self.send(text, window, cx),
            ChatInputEvent::Changed(text) => self.changed(text, cx),
            ChatInputEvent::Complete if self.offer.is_some() => {
                if let Some(index) = self.list.read(cx).active() {
                    self.accept(index, window, cx);
                }
            }
            ChatInputEvent::Complete => self.complete_nick(window, cx),
            ChatInputEvent::Cancel => self.show(None, cx),
            ChatInputEvent::Previous => self.navigate(true, window, cx),
            ChatInputEvent::Next => self.navigate(false, window, cx),
            ChatInputEvent::PasteAttachments(_) => self.error(ATTACHMENTS, cx),
        }
    }

    fn send(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let state = &self.hub.read(cx).state;
        let network = state.network(&self.buffer.network);
        let loaded = state.buffer(&self.buffer).is_some_and(|held| {
            held.entries
                .iter()
                .any(|entry| entry.line.id.as_deref() == Some(self.root.as_str()))
        });
        match command(&self.buffer, &self.root, text, loaded, network) {
            Ok(command) => {
                self.hub.update(cx, |hub, _| hub.send(command));
                self.recall.push(text.to_owned());
                self.input.update(cx, |input, cx| {
                    input.set_busy(false, cx);
                    input.clear(window, cx);
                    input.focus(window, cx);
                });
            }
            Err(reason) => self.error(reason, cx),
        }
    }

    fn error(&self, reason: &str, cx: &mut App) {
        self.input
            .update(cx, |input, cx| input.set_error(reason.to_owned(), cx));
    }

    fn revalidate(&mut self, cx: &mut Context<Self>) {
        let text = self.text(cx);
        let network = self.hub.read(cx).state.network(&self.buffer.network);
        let batches = network.is_some_and(|network| connection_batches(&network.caps));
        let blocked =
            check_draft(&text, batches).is_err() || crate::thread::reply_refusal(network).is_some();
        let limits = DraftLimits::new(draft_line_limit(&text, batches), MAX_INLINE_BYTES);
        self.input.update(cx, |input, cx| {
            input.set_limits(Some(limits), cx);
            input.set_submit_blocked(blocked, cx);
        });
    }

    fn facts(&self, cx: &App) -> Facts {
        let hub = self.hub.read(cx);
        Facts::gather(&hub.state, &self.buffer).ranked(&hub.roster(&self.buffer.network, cx))
    }

    fn changed(&mut self, text: &str, cx: &mut Context<Self>) {
        self.revalidate(cx);
        let recalled = self.recall.showing(text);
        self.input
            .update(cx, |input, cx| input.set_recalled(recalled, cx));
        let cursor = self.input.read(cx).cursor(cx);
        let offer = (!is_command(text))
            .then(|| suggest(text, cursor, &self.facts(cx)))
            .flatten();
        self.show(offer, cx);
        // The inspector renders the current value to keep unavailable drafts visible.
        cx.notify();
    }

    fn show(&mut self, offer: Option<Offer>, cx: &mut App) {
        let items = offer.as_ref().map_or_else(Vec::new, |offer| {
            offer
                .choices
                .iter()
                .map(|choice| choice.item.clone())
                .collect()
        });
        self.input
            .update(cx, |input, cx| input.set_completing(!items.is_empty(), cx));
        self.list.update(cx, |list, cx| list.show(items, cx));
        self.offer = offer;
    }

    fn accept(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(offer) = &self.offer else { return };
        let Some(choice) = offer.choices.get(index) else {
            return;
        };
        let (range, insert) = (offer.range.clone(), choice.insert.clone());
        self.show(None, cx);
        self.input.update(cx, |input, cx| {
            input.replace_range(range, &insert, window, cx);
            input.focus(window, cx);
        });
    }

    fn complete_nick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let facts = self.facts(cx);
        let names: Vec<_> = facts
            .people
            .iter()
            .map(|person| person.nick.as_str())
            .collect();
        let input = self.input.read(cx);
        if let Some((range, nick)) =
            super::complete::complete(&input.text(cx), input.cursor(cx), &names)
        {
            self.input.update(cx, |input, cx| {
                input.replace_range(range, &nick, window, cx)
            });
        }
    }

    fn navigate(&mut self, older: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.offer.is_some() {
            self.list
                .update(cx, |list, cx| list.step(if older { -1 } else { 1 }, cx));
        } else if recall::replace(&mut self.recall, &self.input, older, window, cx).is_some() {
            self.revalidate(cx);
        }
    }
}

impl Render for ReplyDraft {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("irc-reply-editor")
            .test_support()
            .w_full()
            .min_w_0()
            .gap_1()
            .child(self.list.clone())
            .child(self.input.clone())
    }
}

fn is_command(text: &str) -> bool {
    text.starts_with('/') && !text.starts_with("//")
}

fn command(
    buffer: &BufferKey,
    root: &str,
    text: &str,
    loaded: bool,
    network: Option<&Network>,
) -> Result<Command, &'static str> {
    if !matches!(buffer.target, Target::Channel(_)) || !loaded {
        return Err(GONE);
    }
    if let Some(refusal) = crate::thread::reply_refusal(network) {
        return Err(refusal);
    }
    let text = text.trim();
    if is_command(text) {
        return Err(COMMAND);
    }
    let batches = network.is_some_and(|network| connection_batches(&network.caps));
    if check_draft(text, batches).is_err() {
        return Err(TOO_LONG);
    }
    Ok(Command::Reply {
        buffer: buffer.clone(),
        msgid: root.to_owned(),
        text: text
            .strip_prefix('/')
            .filter(|text| text.starts_with('/'))
            .unwrap_or(text)
            .to_owned(),
    })
}
