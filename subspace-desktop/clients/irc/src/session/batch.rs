//! Collects batches that need to be interpreted at their closing message.

mod multiline;

use super::{Output, Session, dispatch, history};
use crate::model::Event;
use crate::proto::Message;

pub(super) const MAX_OPEN_BATCHES: usize = 64;
pub(super) const MAX_BATCH_MESSAGES: usize = 10_000;

pub(super) struct Batch {
    opening: Message,
    buffer: Buffer,
}

enum Buffer {
    Transparent,
    Collecting {
        messages: Vec<Message>,
        complete: bool,
    },
    Delivered,
}

fn position(session: &Session, reference: &str) -> Option<usize> {
    session
        .batches
        .iter()
        .position(|batch| id(&batch.opening) == reference)
}

fn id(opening: &Message) -> &str {
    let reference = opening.param(0);
    reference.strip_prefix('+').unwrap_or(reference)
}

pub(super) fn receive(session: &mut Session, message: Message, out: &mut Output) {
    if message.command != "BATCH"
        && let Some(index) = buffered_parent(session, &message)
    {
        append(session, index, std::iter::once(message), true, out);
    } else {
        dispatch::handle(session, &message, out);
    }
}

pub(super) fn batch(session: &mut Session, message: &Message, out: &mut Output) {
    if let Some(reference) = message.param(0).strip_prefix('-') {
        if let Some(batch) =
            position(session, reference).and_then(|index| session.batches.remove(index))
        {
            finish(session, batch, true, out);
        }
        return;
    }
    if let Some(index) = position(session, id(message)) {
        session.batches.remove(index);
    }
    if session.batches.len() == MAX_OPEN_BATCHES
        && let Some(oldest) = session.batches.pop_front()
    {
        finish(session, oldest, false, out);
    }
    let buffer = if matches!(
        message.param(1),
        "chathistory" | "draft/chathistory" | "draft/multiline" | "znc.in/playback"
    ) || buffered_parent(session, message).is_some()
    {
        Buffer::Collecting {
            messages: Vec::new(),
            complete: true,
        }
    } else {
        Buffer::Transparent
    };
    session.batches.push_back(Batch {
        opening: message.clone(),
        buffer,
    });
}

fn buffered_parent(session: &Session, message: &Message) -> Option<usize> {
    let index = position(session, message.tag("batch")?)?;
    (!matches!(session.batches[index].buffer, Buffer::Transparent)).then_some(index)
}

fn append(
    session: &mut Session,
    index: usize,
    incoming: impl ExactSizeIterator<Item = Message>,
    complete: bool,
    out: &mut Output,
) {
    let batch = &mut session.batches[index];
    let Buffer::Collecting {
        messages,
        complete: buffered_complete,
    } = &mut batch.buffer
    else {
        return;
    };
    *buffered_complete &= complete;
    let room = MAX_BATCH_MESSAGES - messages.len();
    let overflow = incoming.len() > room;
    messages.extend(incoming.take(room));
    if overflow {
        // Keep recognising the reference until its close so excess history
        // and multiline components do not escape as live messages.
        let batch = Batch {
            opening: batch.opening.clone(),
            buffer: std::mem::replace(&mut batch.buffer, Buffer::Delivered),
        };
        finish(session, batch, false, out);
    }
}

fn finish(session: &mut Session, batch: Batch, closed: bool, out: &mut Output) {
    let Batch { opening, buffer } = batch;
    let Buffer::Collecting {
        mut messages,
        complete,
    } = buffer
    else {
        return;
    };
    let complete = complete && closed;
    let kind = opening.param(1);
    if kind == "draft/multiline" {
        messages = multiline::combine(&opening, messages).into_iter().collect();
    }
    if let Some(index) = buffered_parent(session, &opening) {
        append(
            session,
            index,
            std::mem::take(&mut messages).into_iter(),
            complete,
            out,
        );
    }
    if matches!(kind, "chathistory" | "draft/chathistory") {
        history::deliver(session, opening.param(2), &messages, complete, out);
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
