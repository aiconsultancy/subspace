//! Sending text: line splitting, CTCP `ACTION`, `draft/multiline` batches
//! and local echo when the server has no `echo-message`.

use super::split::{budget, split_multiline_text, split_text};
use super::{Output, Session, lines};
use crate::model::{Event, LineKind, Target};
use crate::proto::Message;

#[cfg(test)]
mod limit_tests;

const ECHO: &str = "echo-message";
const MULTILINE: &str = "draft/multiline";
const CONCAT: &str = "draft/multiline-concat";
const ACTION: &str = "ACTION";
pub(super) const REPLY: &str = "+draft/reply";
/// `draft/multiline` limits when the server gives none.
const DEFAULT_MAX_BYTES: usize = 4096;
const DEFAULT_MAX_LINES: usize = 100;
/// Bound the work a draft can queue, including on networks without multiline.
/// A connection with `draft/multiline` sends text as one batch, so it may
/// carry more lines than text sent line by line.
pub const MAX_INLINE_BYTES: usize = 4096;
const MAX_INLINE_LINES: usize = 10;
const MAX_BATCHED_LINES: usize = 100;

/// Bytes of text one `PRIVMSG` to `target` carries in a single relayed line,
/// as the client splits it: an app packing several items into lines uses
/// this so the client never has to split one. An empty `username` falls back
/// to `nick`, as registration does.
pub fn message_budget(nick: &str, username: &str, target: &str) -> usize {
    let user = if username.is_empty() { nick } else { username };
    budget("PRIVMSG", nick, user, target, 0)
}

/// Checks the client-wide inline limit before preparing any protocol messages.
/// `batched` raises the line limit for a connection with `draft/multiline`.
/// Within the limit, sends still use the server's negotiated limits to choose
/// batching.
pub fn check_inline_text(text: &str, batched: bool) -> Result<(), String> {
    check_limited_text(text, inline_line_limit(batched))
}

fn check_limited_text(text: &str, lines: usize) -> Result<(), String> {
    if text.len() > MAX_INLINE_BYTES || text.lines().nth(lines).is_some() {
        return Err(format!(
            "IRC messages can be up to {lines} lines and {MAX_INLINE_BYTES} bytes."
        ));
    }
    Ok(())
}

/// Checks composer input, where a leading `/` starts a command (`//` is
/// literal text). Plain text on a connection with `draft/multiline` may use
/// the batched limit; commands keep the line-by-line limit, since `/quote`
/// sends raw lines.
pub fn check_draft(text: &str, connection_batches: bool) -> Result<(), String> {
    check_limited_text(text, draft_line_limit(text, connection_batches))
}

/// The composer's actual line cap, including the command exception.
pub fn draft_line_limit(text: &str, connection_batches: bool) -> usize {
    inline_line_limit(connection_batches && !super::input::is_command(text))
}

fn inline_line_limit(batched: bool) -> usize {
    if batched {
        MAX_BATCHED_LINES
    } else {
        MAX_INLINE_LINES
    }
}

/// Whether a connection with these enabled caps sends multi-line text as
/// `draft/multiline` batches.
pub fn connection_batches(enabled: &[String]) -> bool {
    enabled.iter().any(|cap| cap == MULTILINE)
}

/// Whether this connection sends multi-line text as batches.
pub(super) fn batches(session: &Session) -> bool {
    session.has_cap(MULTILINE)
}

/// Uses normal splitting and echo, tagging each standalone message or only
/// the opening multiline batch. Tag values use the protocol serializer.
pub(super) fn reply(
    session: &mut Session,
    target: &Target,
    msgid: &str,
    text: &str,
    out: &mut Output,
) -> Result<Vec<Message>, String> {
    if !session.has_cap("message-tags") || msgid.is_empty() {
        return Err("Replies require message-tags and a message ID".into());
    }
    let tag = crate::proto::serialise(&Message::new("PRIVMSG", ["x"]).with_tag(REPLY, msgid));
    let tag_bytes = tag.split_once(' ').map_or(0, |(tags, _)| tags.len() + 1);
    if msgid.contains('\0') || tag_bytes > crate::codec::CLIENT_TAGS_LIMIT {
        return Err("The reply message ID cannot be sent as an IRC tag".into());
    }
    check_inline_text(text, batches(session))?;
    let start = out.events.len();
    let messages = say(session, target, LineKind::Privmsg, text, out)?;
    out.events[start..].iter_mut().for_each(|event| {
        if let Event::Line(line) = event {
            line.reply_to = Some(msgid.to_string());
        }
    });
    Ok(messages
        .into_iter()
        .map(|message| tag_reply(message, msgid))
        .collect())
}

fn tag_reply(message: Message, msgid: &str) -> Message {
    let opening = message.command == "BATCH" && message.param(0).starts_with('+');
    let single = message.command == "PRIVMSG" && message.tag("batch").is_none();
    match opening || single {
        true => message.with_tag(REPLY, msgid),
        false => message,
    }
}

/// Text for the buffer's own target.
pub(super) fn say(
    session: &mut Session,
    target: &Target,
    kind: LineKind,
    text: &str,
    out: &mut Output,
) -> Result<Vec<Message>, String> {
    let name = match target {
        Target::Channel(name) | Target::Query(name) => name.clone(),
        Target::Server => {
            return Err("The server buffer has no conversation; use /msg or /quote".to_string());
        }
    };
    to(session, &name, kind, text, out)
}

/// Text for `name` (a channel or nick), split to fit.
pub(super) fn to(
    session: &mut Session,
    name: &str,
    kind: LineKind,
    text: &str,
    out: &mut Output,
) -> Result<Vec<Message>, String> {
    let target = session.isupport.channel(name).map_or_else(
        || Target::Query(name.to_string()),
        |channel| Target::Channel(channel.to_string()),
    );
    let (command, ctcp) = wrapper(kind);
    let user = match session.config.username.is_empty() {
        true => session.reg.nick.clone(),
        false => session.config.username.clone(),
    };
    let extra = ctcp.map_or(0, |command| command.len() + "\u{1} \u{1}".len());
    let room = budget(command, &session.reg.nick, &user, name, extra);
    let limits = session
        .caps
        .offered
        .get(MULTILINE)
        .filter(|_| session.has_cap(MULTILINE) && kind == LineKind::Privmsg)
        .cloned();
    let text_lines: Vec<&str> = text.lines().collect();
    let pieces: Vec<Vec<String>> = text_lines
        .iter()
        .map(|line| match limits {
            Some(_) => split_multiline_text(line, room),
            None => split_text(line, room),
        })
        .collect();
    let messages = match limits.filter(|_| wire_lines(&pieces) > 1) {
        Some(limits) => batched(session, name, &limits, &pieces, room)?,
        None if text_lines.len() > MAX_INLINE_LINES => {
            return Err(format!(
                "Actions and notices can be up to {MAX_INLINE_LINES} lines."
            ));
        }
        None => singles(command, ctcp, name, &pieces),
    };
    echo(session, &target, kind, &text_lines, out);
    Ok(messages)
}

/// A fresh reference for a batch we send.
fn reference(sent: &mut u64) -> String {
    *sent += 1;
    format!("subspace{sent}")
}

fn wrapper(kind: LineKind) -> (&'static str, Option<&'static str>) {
    match kind {
        LineKind::Notice => ("NOTICE", None),
        LineKind::Action => ("PRIVMSG", Some(ACTION)),
        _ => ("PRIVMSG", None),
    }
}

/// Text in as few `draft/multiline` batches as the server's limits allow, so
/// text the composer accepted is sent, not refused. One physical line or an
/// oversized logical line goes as plain messages. More groups than the
/// line-by-line limit (a server with tiny batches) are refused.
fn batched(
    session: &mut Session,
    name: &str,
    limits: &str,
    pieces: &[Vec<String>],
    room: usize,
) -> Result<Vec<Message>, String> {
    let groups = batch_groups(limits, pieces);
    if groups.len() > MAX_INLINE_LINES {
        return Err(format!(
            "This server's batches are too small for this message. Send at most {MAX_INLINE_LINES} lines."
        ));
    }
    Ok(groups
        .into_iter()
        .flat_map(
            |group| match wire_lines(group) > 1 && fits_batch(limits, group) {
                true => batch(&reference(&mut session.sent_batches), name, group),
                false => unbatched(name, group, room),
            },
        )
        .collect())
}

/// Reapply the legacy word splitting when a negotiated group cannot batch.
fn unbatched(name: &str, group: &[Vec<String>], room: usize) -> Vec<Message> {
    let pieces: Vec<Vec<String>> = group
        .iter()
        .map(|line| split_text(&line.concat(), room))
        .collect();
    singles("PRIVMSG", None, name, &pieces)
}

fn wire_lines(pieces: &[Vec<String>]) -> usize {
    pieces.iter().map(|line| line.len().max(1)).sum()
}

/// Consecutive lines, grouped greedily so each group fits one batch.
fn batch_groups<'a>(limits: &str, pieces: &'a [Vec<String>]) -> Vec<&'a [Vec<String>]> {
    let mut groups = Vec::new();
    let mut start = 0;
    while start < pieces.len() {
        let mut end = start + 1;
        while end < pieces.len() && fits_batch(limits, &pieces[start..=end]) {
            end += 1;
        }
        groups.push(&pieces[start..end]);
        start = end;
    }
    groups
}

/// Whether these lines fit one `draft/multiline` batch (`limits` is the
/// cap's value, such as `max-bytes=4096,max-lines=100`).
fn fits_batch(limits: &str, pieces: &[Vec<String>]) -> bool {
    let limit = |key: &str, default: usize| {
        limits
            .split(',')
            .filter_map(|pair| pair.split_once('='))
            .find(|(name, _)| *name == key)
            .and_then(|(_, value)| value.parse().ok())
            .unwrap_or(default)
    };
    let count = wire_lines(pieces);
    let bytes: usize = pieces.iter().flatten().map(String::len).sum::<usize>() + pieces.len() - 1;
    count <= limit("max-lines", DEFAULT_MAX_LINES) && bytes <= limit("max-bytes", DEFAULT_MAX_BYTES)
}

/// One message per piece; empty lines are skipped.
fn singles(
    command: &str,
    ctcp: Option<&str>,
    target: &str,
    pieces: &[Vec<String>],
) -> Vec<Message> {
    pieces
        .iter()
        .flatten()
        .map(|piece| {
            let text = match ctcp {
                Some(ctcp) => format!("\u{1}{ctcp} {piece}\u{1}"),
                None => piece.clone(),
            };
            Message::new(command, [target.to_string(), text])
        })
        .collect()
}

/// A `draft/multiline` batch: each line's continuation pieces carry
/// `draft/multiline-concat`; empty lines are kept.
fn batch(reference: &str, target: &str, pieces: &[Vec<String>]) -> Vec<Message> {
    let open = Message::new(
        "BATCH",
        [format!("+{reference}"), MULTILINE.into(), target.into()],
    );
    let close = Message::new("BATCH", [format!("-{reference}")]);
    let body = pieces.iter().flat_map(|line| {
        let line = match line.is_empty() {
            true => vec![String::new()],
            false => line.clone(),
        };
        line.into_iter().enumerate().map(|(index, piece)| {
            let message =
                Message::new("PRIVMSG", [target.to_string(), piece]).with_tag("batch", reference);
            match index {
                0 => message,
                _ => message.with_tag(CONCAT, ""),
            }
        })
    });
    std::iter::once(open)
        .chain(body)
        .chain(std::iter::once(close))
        .collect()
}

/// Our lines, when the server will not echo them.
fn echo(session: &Session, target: &Target, kind: LineKind, text_lines: &[&str], out: &mut Output) {
    let echoed = session.has_cap(ECHO);
    let local = text_lines
        .iter()
        .filter(|line| !echoed && !line.is_empty())
        .map(|line| {
            Event::Line(lines::local_line(
                session,
                target.clone(),
                kind,
                line.to_string(),
            ))
        });
    out.events.extend(local);
}
