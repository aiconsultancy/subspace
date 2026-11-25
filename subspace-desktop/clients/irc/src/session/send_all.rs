//! `Command::SendAll`: every line is checked before any is written, and
//! only once registered.

use super::input::{Input, parse_input};
use super::{Output, Session, commands, send};
use crate::model::{BufferKey, Command, Event, LineKind, SendResult};

/// Why a `SendAll` waits for registration.
pub(super) const NOT_REGISTERED: &str = "Not connected yet. Your message is kept.";
/// Why a `SendAll` may have partly gone.
pub(super) const WRITE_FAILED: &str = "Connection lost while sending; some lines may have gone. Check the channel before sending again.";

enum SendLines<'a> {
    Separate(&'a [String]),
    Together(&'a [String]),
}

pub(super) fn command(session: &mut Session, command: Command, out: &mut Output) {
    let (buffer, id, lines) = match &command {
        Command::SendAll { buffer, id, lines } => (buffer, *id, SendLines::Separate(lines)),
        Command::SendAllTogether { buffer, id, lines } => (buffer, *id, SendLines::Together(lines)),
        _ => return,
    };
    send_all(session, buffer, id, lines, out);
}

/// Checked before written: the connection must be registered, and each line
/// is checked and prepared against its negotiated limits first. Only then
/// do all go to the connection; their local echoes and the success answer
/// follow a full write, an uncertain answer a failed one. Always answered
/// with [`Event::Sent`].
fn send_all(
    session: &mut Session,
    buffer: &BufferKey,
    id: u64,
    lines: SendLines<'_>,
    out: &mut Output,
) {
    let mut prepared = Output::default();
    let result = registered(session).and_then(|()| match lines {
        SendLines::Separate(lines) => prepare_all(session, buffer, lines, &mut prepared),
        SendLines::Together(lines) => prepare_together(session, buffer, lines, &mut prepared),
    });
    let sent = |result| Event::Sent {
        buffer: buffer.clone(),
        id,
        result,
    };
    match result {
        Ok(()) => {
            out.send.extend(prepared.send);
            out.written.extend(prepared.events);
            out.written.push(sent(SendResult::Written));
            out.unwritten
                .push(sent(SendResult::Uncertain(WRITE_FAILED.into())));
        }
        Err(error) => {
            commands::error_line(session, &buffer.target, error.clone(), out);
            out.event(sent(SendResult::Refused(error)));
        }
    }
}

/// Lines go only once the server welcomed us: before that, their checks
/// and echoes would use capabilities not yet agreed.
fn registered(session: &Session) -> Result<(), String> {
    match session.registered() {
        true => Ok(()),
        false => Err(NOT_REGISTERED.to_string()),
    }
}

/// Every line of a `SendAll`, prepared into `prepared`, or why
/// one cannot go.
fn prepare_all(
    session: &mut Session,
    buffer: &BufferKey,
    lines: &[String],
    prepared: &mut Output,
) -> Result<(), String> {
    lines.iter().try_for_each(|line| {
        let text = match parse_input(line)? {
            Input::Say(text) => text,
            _ => return Err("Send commands without attachments.".to_string()),
        };
        send::check_draft(&text, send::batches(session))?;
        let messages = send::say(session, &buffer.target, LineKind::Privmsg, &text, prepared)?;
        prepared.send.extend(messages);
        Ok(())
    })
}

/// The composer has already packed the upload references into whole lines.
/// Feed them into the normal multiline sender with the draft, then accept
/// its prepared output only if it contains one logical IRC message.
fn prepare_together(
    session: &mut Session,
    buffer: &BufferKey,
    lines: &[String],
    prepared: &mut Output,
) -> Result<(), String> {
    prepare_all(session, buffer, &[lines.join("\n")], prepared)?;
    let logical_messages = prepared
        .send
        .iter()
        .filter(|message| {
            (message.command == "PRIVMSG" && message.tag("batch").is_none())
                || (message.command == "BATCH" && message.param(0).starts_with('+'))
        })
        .count();
    if logical_messages == 1 {
        Ok(())
    } else {
        Err("This server cannot send this message and its attachments together. Shorten the message and try again.".into())
    }
}
