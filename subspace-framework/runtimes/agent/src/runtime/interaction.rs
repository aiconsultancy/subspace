//! The event queue the UI drains, with every event tagged by chat.
use super::{DELTA_LIMIT, EVENT_LIMIT, contains_secret, lock, redact};
use crate::{ChatEvent, ChatId, Entry, EntryKind, Event, TranscriptEntry};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use subspace_core::Secret;
use tokio::sync::Notify;

/// The unfinished word of one stream: a chat's reply and its reasoning are
/// buffered separately, so neither mixes with another chat's.
type StreamKey = (ChatId, bool);

#[derive(Default)]
pub(super) struct EventQueue {
    pub(super) items: Mutex<VecDeque<Event>>,
    pub(super) space: Notify,
    pub(super) stopped: AtomicBool,
    /// The failure that stopped the runtime, redacted.
    failure: Mutex<Option<String>>,
    secrets: Mutex<Vec<Secret>>,
    streaming_words: Mutex<HashMap<StreamKey, String>>,
}

/// The delta of `chat` that `event` would interrupt, if it is a final row.
fn flushes_stream(event: &Event) -> Option<ChatId> {
    let Event::Chat { chat, event } = event else {
        return None;
    };
    matches!(
        event,
        ChatEvent::Entry(_) | ChatEvent::History(_) | ChatEvent::TurnDone
    )
    .then_some(*chat)
}

impl EventQueue {
    pub(super) async fn push(&self, event: Event) {
        if let Some(chat) = flushes_stream(&event) {
            lock(&self.streaming_words).retain(|(id, _), _| *id != chat);
        }
        let mut event = Some(self.sanitize(event));
        loop {
            let space = self.space.notified();
            {
                let mut queue = lock(&self.items);
                if queue.len() >= EVENT_LIMIT
                    && let Some(index) = queue.iter().position(is_delta)
                {
                    queue.remove(index);
                }
                if queue.len() < EVENT_LIMIT {
                    if let Some(event) = event.take() {
                        queue.push_back(event);
                    }
                    return;
                }
            }
            // Lossless events apply backpressure; only interim deltas may be dropped.
            space.await;
        }
    }

    /// The completed words of one chat's stream, or nothing while the word is
    /// still arriving.
    fn completed_words(&self, chat: ChatId, thinking: bool, text: &str) -> Option<String> {
        // Keep the unfinished word so a token split across SDK chunks is never
        // exposed a character at a time. Authoritative final entries supply the tail.
        let mut words = lock(&self.streaming_words);
        let pending = words.entry((chat, thinking)).or_default();
        pending.push_str(text);
        if pending.len() > DELTA_LIMIT {
            pending.clear();
            return None;
        }
        let (index, character) = pending
            .char_indices()
            .rfind(|(_, character)| character.is_whitespace())?;
        let completed: String = pending.drain(..index + character.len_utf8()).collect();
        Some(self.redact(&completed))
    }

    pub(super) fn delta(&self, chat: ChatId, kind: EntryKind, text: String) {
        let thinking = kind == EntryKind::Thinking;
        let Some(text) = self.completed_words(chat, thinking, &text) else {
            return;
        };
        let mut queue = lock(&self.items);
        if let Some(Event::Chat {
            chat: open,
            event:
                ChatEvent::Delta {
                    kind: previous_kind,
                    text: previous,
                },
        }) = queue.back_mut()
            && *open == chat
            && *previous_kind == kind
            && previous.len() + text.len() <= DELTA_LIMIT
        {
            previous.push_str(&text);
        } else if queue.len() < EVENT_LIMIT && text.len() <= DELTA_LIMIT {
            queue.push_back(Event::of(chat, ChatEvent::Delta { kind, text }));
        }
    }

    pub(super) fn push_initial_error(&self, message: &str) {
        *lock(&self.failure) = Some(message.into());
        lock(&self.items).push_back(Event::Error(message.into()));
    }

    /// Reports the failure that stopped the runtime, and keeps it for a
    /// command sent afterwards.
    pub(super) async fn fail(&self, message: &str) {
        *lock(&self.failure) = Some(self.redact(message));
        self.push(Event::Error(message.into())).await;
    }

    pub(super) fn failure(&self) -> Option<String> {
        lock(&self.failure).clone()
    }

    pub(super) fn remember_secrets(&self, secrets: impl IntoIterator<Item = Secret>) {
        lock(&self.secrets).extend(
            secrets
                .into_iter()
                .filter(|secret| !secret.expose().is_empty()),
        );
    }

    pub(super) fn has_secret(&self, text: &str) -> bool {
        contains_secret(text)
            || lock(&self.secrets)
                .iter()
                .any(|secret| text.contains(secret.expose()))
    }

    /// Masks the credentials the runtime knows in every text of a failure.
    pub(super) fn clean_failure(&self, failure: &mut crate::TurnFailure) {
        failure.redact_with(|text| self.redact(text));
    }

    fn redact(&self, text: &str) -> String {
        let mut text = redact(text);
        for secret in lock(&self.secrets).iter() {
            text = text.replace(secret.expose(), "[redacted]");
        }
        text
    }

    fn sanitize(&self, mut event: Event) -> Event {
        match &mut event {
            Event::Chat { event, .. } => self.sanitize_chat(event),
            Event::Error(text) | Event::Notice(text) => *text = self.redact(text),
            Event::Models(items) => self.each(items.iter_mut().map(|item| &mut item.label)),
            Event::Agents(items) => self.each(items.iter_mut().map(|item| &mut item.label)),
            Event::Sessions(items) => self.each(items.iter_mut().map(|item| &mut item.title)),
            Event::Commands(items) => self.each(items.iter_mut().map(|item| &mut item.description)),
            Event::Restore { .. } | Event::SignInRequired(..) => {}
        }
        event
    }

    fn sanitize_chat(&self, event: &mut ChatEvent) {
        match event {
            ChatEvent::Error(text)
            | ChatEvent::SelectedAgent(text)
            | ChatEvent::SelectedModel { model: text, .. }
            | ChatEvent::Title(text)
            | ChatEvent::Delta { text, .. } => *text = self.redact(text),
            ChatEvent::Entry(entry) => self.redact_entry(entry),
            ChatEvent::Failure(failure) => self.clean_failure(failure),
            ChatEvent::Approval { summary, .. } => *summary = self.redact(summary),
            ChatEvent::History(items) => {
                for item in items.iter_mut() {
                    self.redact_entry(item);
                }
            }
            ChatEvent::Question {
                question, choices, ..
            } => self.each(std::iter::once(question).chain(choices.iter_mut())),
            _ => {}
        }
    }

    /// Every string a row shows passes through redaction, including a tool
    /// call's title, argument summary and result.
    fn redact_entry(&self, entry: &mut TranscriptEntry) {
        match &mut entry.entry {
            Entry::Assistant(text) | Entry::Info(text) => *text = self.redact(text),
            Entry::User { text, .. } => *text = self.redact(text),
            Entry::Thinking { text, .. } => *text = self.redact(text),
            Entry::Tool(call) => {
                call.title = self.redact(&call.title);
                call.detail = self.redact(&call.detail);
                call.result = self.redact(&call.result);
            }
        }
    }

    fn each<'a>(&self, values: impl Iterator<Item = &'a mut String>) {
        for value in values {
            *value = self.redact(value);
        }
    }
}

fn is_delta(event: &Event) -> bool {
    matches!(
        event,
        Event::Chat {
            event: ChatEvent::Delta { .. },
            ..
        }
    )
}
