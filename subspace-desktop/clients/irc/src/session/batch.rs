//! Collects batches that need to be interpreted at their closing message.

use super::{Output, Session, dispatch, history, send::REPLY};
use crate::model::Event;
use crate::proto::Message;

const CONCAT: &str = "draft/multiline-concat";

pub(super) struct Batch {
    opening: Message,
    buffered: bool,
    messages: Vec<Message>,
}

pub(super) fn receive(session: &mut Session, message: Message, out: &mut Output) {
    if message.command != "BATCH"
        && let Some(reference) = message.tag("batch")
        && let Some(batch) = session.batches.get_mut(reference)
        && batch.buffered
    {
        batch.messages.push(message);
    } else {
        dispatch::handle(session, &message, out);
    }
}

pub(super) fn batch(session: &mut Session, message: &Message, out: &mut Output) {
    let reference = message.param(0);
    if let Some(reference) = reference.strip_prefix('-') {
        close(session, reference, out);
    } else {
        let reference = reference.strip_prefix('+').unwrap_or(reference).to_owned();
        let buffered = matches!(
            message.param(1),
            "chathistory" | "draft/chathistory" | "draft/multiline" | "znc.in/playback"
        ) || message
            .tag("batch")
            .and_then(|parent| session.batches.get(parent))
            .is_some_and(|parent| parent.buffered);
        session.batches.insert(
            reference,
            Batch {
                opening: message.clone(),
                buffered,
                messages: Vec::new(),
            },
        );
    }
}

fn close(session: &mut Session, reference: &str, out: &mut Output) {
    let Some(Batch {
        opening,
        mut messages,
        ..
    }) = session.batches.remove(reference)
    else {
        return;
    };
    let kind = opening.param(1);
    if kind == "draft/multiline" {
        messages = combine(&opening, messages).into_iter().collect();
    }
    if let Some(parent) = opening
        .tag("batch")
        .and_then(|parent| session.batches.get_mut(parent))
        && parent.buffered
    {
        parent.messages.append(&mut messages);
    }
    if matches!(kind, "chathistory" | "draft/chathistory") {
        history::deliver(session, opening.param(2), &messages, out);
        return;
    }

    let start = out.events.len();
    for message in &messages {
        dispatch::handle(session, message, out);
    }
    if kind == "znc.in/playback" {
        for event in &mut out.events[start..] {
            if let Event::Line(line) = event {
                *event = Event::Replayed(line.clone());
            }
        }
    }
}

fn combine(opening: &Message, messages: Vec<Message>) -> Option<Message> {
    let mut messages = messages.into_iter();
    let mut first = messages.next()?;
    let target = first.param(0).to_owned();
    let mut text = first.trailing().to_owned();
    for message in messages {
        if message.tag(CONCAT).is_none() {
            text.push('\n');
        }
        text.push_str(message.trailing());
    }
    first.params = vec![target, text];
    first.tags.remove(REPLY);
    first.tags.extend(opening.tags.clone());
    first.tags.remove("batch");
    first.tags.remove(CONCAT);
    Some(first)
}
