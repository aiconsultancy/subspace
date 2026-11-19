//! The checks that need no connection: sizes, names and text.

use subspace_client_irc::check_inline_text;

use super::Write;
use crate::editor::validate::{is_channel, is_nick};

const MAX_OPERATIONS: usize = 10;
pub(super) const MAX_TEXT_CHARS: usize = 2000;
const MAX_KEY_CHARS: usize = 50;
const MAX_REASON_CHARS: usize = 300;

/// The checks that need no connection.
pub(crate) fn validate(operations: &[Write]) -> Result<(), String> {
    if operations.len() > MAX_OPERATIONS {
        return Err(format!("At most {MAX_OPERATIONS} IRC operations per call."));
    }
    operations
        .iter()
        .enumerate()
        .try_for_each(|(index, operation)| numbered(index, validate_one(operation)))
}

pub(super) fn numbered<T>(index: usize, result: Result<T, String>) -> Result<T, String> {
    result.map_err(|error| format!("Operation {}: {error}", index + 1))
}

fn validate_one(operation: &Write) -> Result<(), String> {
    if operation.network().trim().is_empty() {
        return Err("network is required.".into());
    }
    match operation {
        Write::Message {
            target, text, me, ..
        } => recipient(target).and_then(|()| message_text(text, *me)),
        Write::Join { channel, key, .. } => {
            channel_name(channel).and_then(|()| key.as_deref().map_or(Ok(()), channel_key))
        }
        Write::Part {
            channel, reason, ..
        } => channel_name(channel).and_then(|()| reason.as_deref().map_or(Ok(()), part_reason)),
        Write::Connect { .. } | Write::Disconnect { .. } => Ok(()),
    }
}

/// A channel, or a nick that is not a network service: services such as
/// NickServ change accounts and channels.
fn recipient(target: &str) -> Result<(), String> {
    if is_channel(target) {
        return channel_name(target);
    }
    if !is_nick(target) {
        return Err(format!("{target} is not a channel or a nick."));
    }
    match target.to_ascii_lowercase().ends_with("serv") {
        true => Err(format!(
            "{target} looks like a network service; tools do not message services."
        )),
        false => Ok(()),
    }
}

fn message_text(text: &str, me: bool) -> Result<(), String> {
    let lines = text.lines().count();
    let problems = [
        (text.trim().is_empty(), "text is empty.".to_owned()),
        (
            text.chars().count() > MAX_TEXT_CHARS,
            format!("text is longer than {MAX_TEXT_CHARS} characters."),
        ),
        (
            text.chars().any(|c| c.is_control() && c != '\n' && c != '\t'),
            "text may hold line breaks and tabs, but no other control characters such as CTCP or formatting codes.".to_owned(),
        ),
        (me && lines > 1, "An action is one line.".to_owned()),
    ];
    problems
        .into_iter()
        .find(|(failed, _)| *failed)
        .map_or_else(
            || check_inline_text(text, false),
            |(_, problem)| Err(problem),
        )
}

fn channel_name(channel: &str) -> Result<(), String> {
    match is_channel(channel) && !channel.chars().any(char::is_control) {
        true => Ok(()),
        false => Err(format!(
            "{channel} is not a channel name. Channel names start with # or &."
        )),
    }
}

fn channel_key(key: &str) -> Result<(), String> {
    let plain = !key.is_empty()
        && key.chars().count() <= MAX_KEY_CHARS
        && !key
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == ',');
    match plain {
        true => Ok(()),
        false => Err(format!(
            "The channel key must be 1 to {MAX_KEY_CHARS} characters without spaces or commas."
        )),
    }
}

fn part_reason(reason: &str) -> Result<(), String> {
    match reason.chars().count() <= MAX_REASON_CHARS && !reason.chars().any(char::is_control) {
        true => Ok(()),
        false => Err(format!(
            "The reason must be one line of at most {MAX_REASON_CHARS} characters."
        )),
    }
}
