//! `Command::SendAll`: every line is checked before any is written, and
//! only once registered.

use super::input::{Input, parse_input};
use super::{Output, Session, commands, send};
use crate::model::{BufferKey, Event, LineKind, SendResult};

/// Why a `SendAll` waits for registration.
pub(super) const NOT_REGISTERED: &str = "Not connected yet. Your message is kept.";
/// Why a `SendAll` may have partly gone.
pub(super) const WRITE_FAILED: &str = "Connection lost while sending; some lines may have gone. Check the channel before sending again.";

/// Checked before written: the connection must be registered, and each line
/// is checked and prepared against its negotiated limits first. Only then
/// do all go to the connection; their local echoes and the success answer
/// follow a full write, an uncertain answer a failed one. Always answered
/// with [`Event::Sent`].
pub(super) fn send_all(
    session: &mut Session,
    buffer: &BufferKey,
    id: u64,
    lines: &[String],
    out: &mut Output,
) {
    let mut prepared = Output::default();
    let result =
        registered(session).and_then(|()| prepare_all(session, buffer, lines, &mut prepared));
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
