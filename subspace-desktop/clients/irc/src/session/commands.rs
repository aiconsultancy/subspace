//! Owner commands: composer input, joins, history, read markers, typing.

use chrono::{DateTime, Utc};

use super::input::{Input, parse_input};
use super::{Output, Session, history, lines, listing, membership, send};
use crate::model::{BufferKey, Command, Event, LineKind, Target, Typing};
use crate::proto::{Message, parse};

const READ_MARKER: &str = "draft/read-marker";
const MESSAGE_TAGS: &str = "message-tags";

pub(super) fn handle(session: &mut Session, command: Command, out: &mut Output) {
    match command {
        Command::Input { buffer, text } => input(session, &buffer.target, &text, out),
        Command::Say {
            buffer,
            text,
            action,
        } => say(session, &buffer.target, &text, action, out),
        Command::SendAll { buffer, id, lines } => {
            super::send_all::send_all(session, &buffer, id, &lines, out);
        }
        Command::Reply {
            buffer,
            msgid,
            text,
        } => match send::reply(session, &buffer.target, &msgid, &text, out) {
            Ok(messages) => out.send.extend(messages),
            Err(error) => error_line(session, &buffer.target, error, out),
        },
        Command::SetAutojoin { channels, .. } => super::autojoin::replace(session, channels, out),
        Command::Join { channel, key, .. } => out.send(join(&channel, key)),
        Command::Part { buffer, reason } => out
            .send
            .extend(part(&buffer.target, None, &reason).unwrap_or_default()),
        Command::Forget { buffer } => membership::forget(session, &buffer.target),
        Command::Query { nick, .. } => {
            let opened = query(session, &nick, None, out);
            out.send.extend(opened.unwrap_or_default());
        }
        Command::LoadHistory {
            buffer,
            before,
            limit,
        } => load_history(session, &buffer, before, limit, out),
        Command::LoadAround {
            buffer,
            id,
            time,
            limit,
        } => load_around(session, &buffer, (id.as_deref(), time), limit, out),
        Command::Anchors { anchors, .. } => anchors
            .into_iter()
            .for_each(|anchor| history::remember(&mut session.anchors, anchor)),
        Command::MarkRead { buffer, time } => {
            out.send.extend(mark_read(session, &buffer.target, time))
        }
        Command::Typing { buffer, state } => {
            out.send.extend(typing(session, &buffer.target, state))
        }
        Command::Disconnect { reason, .. } => {
            let messages = quit(reason, out);
            out.send.extend(messages);
        }
        Command::ListChannels { .. } => listing::request(session, out),
        Command::Configure(_) | Command::Connect(_) => {}
    }
}

fn input(session: &mut Session, target: &Target, text: &str, out: &mut Output) {
    let sent = send::check_draft(text, send::batches(session))
        .and_then(|()| parse_input(text))
        .and_then(|input| run(session, target, input, out));
    let failed = sent.as_ref().err().cloned();
    out.send.extend(sent.unwrap_or_default());
    failed
        .into_iter()
        .for_each(|error| error_line(session, target, error, out));
}

/// Literal text: a leading `/` is part of the message.
fn say(session: &mut Session, target: &Target, text: &str, action: bool, out: &mut Output) {
    let kind = if action {
        LineKind::Action
    } else {
        LineKind::Privmsg
    };
    let batched = send::batches(session);
    match send::check_inline_text(text, batched)
        .and_then(|()| send::say(session, target, kind, text, out))
    {
        Ok(messages) => out.send.extend(messages),
        Err(error) => error_line(session, target, error, out),
    }
}

pub(super) fn error_line(session: &Session, target: &Target, error: String, out: &mut Output) {
    out.event(Event::Line(lines::local_line(
        session,
        target.clone(),
        LineKind::Error,
        error,
    )));
}

fn run(
    session: &mut Session,
    target: &Target,
    input: Input,
    out: &mut Output,
) -> Result<Vec<Message>, String> {
    let channel = channel_name(target).map(str::to_string);
    match input {
        Input::Say(text) => send::say(session, target, LineKind::Privmsg, &text, out),
        Input::Me(text) => send::say(session, target, LineKind::Action, &text, out),
        Input::Msg { target, text } => send::to(session, &target, LineKind::Privmsg, &text, out),
        Input::Notice { target, text } => send::to(session, &target, LineKind::Notice, &text, out),
        Input::Query { nick, text } => query(session, &nick, text, out),
        Input::Join { channel, key } => Ok(vec![join(&channel, key)]),
        Input::Part {
            channel: named,
            reason,
        } => part(target, named, &reason),
        Input::Topic {
            channel: named,
            text,
        } => topic(named.or(channel), text),
        Input::Kick {
            channel: named,
            nick,
            reason,
        } => kick(named.or(channel), nick, reason),
        Input::Invite {
            nick,
            channel: named,
        } => invite(nick, named.or(channel)),
        Input::Mode(args) => Ok(vec![mode(channel, &session.reg.nick, args)]),
        Input::Nick(nick) => Ok(vec![Message::new("NICK", [nick])]),
        Input::Quit(reason) => Ok(quit(reason, out)),
        Input::Raw(line) => parse(&line)
            .map(|message| vec![message])
            .map_err(|error| error.to_string()),
    }
}

fn channel_name(target: &Target) -> Option<&str> {
    match target {
        Target::Channel(name) => Some(name),
        _ => None,
    }
}

fn join(channel: &str, key: Option<String>) -> Message {
    Message::new("JOIN", std::iter::once(channel.to_string()).chain(key))
}

fn part(target: &Target, named: Option<String>, reason: &str) -> Result<Vec<Message>, String> {
    let channel = named
        .or_else(|| channel_name(target).map(str::to_string))
        .ok_or("Use /part in a channel, or name one: /part #channel")?;
    let reason = (!reason.is_empty()).then(|| reason.to_string());
    Ok(vec![Message::new(
        "PART",
        std::iter::once(channel).chain(reason),
    )])
}

fn topic(channel: Option<String>, text: Option<String>) -> Result<Vec<Message>, String> {
    let channel = channel.ok_or("Use /topic in a channel, or name one: /topic #channel")?;
    Ok(vec![Message::new(
        "TOPIC",
        std::iter::once(channel).chain(text),
    )])
}

fn kick(channel: Option<String>, nick: String, reason: String) -> Result<Vec<Message>, String> {
    let channel = channel.ok_or("Use /kick in a channel, or name one: /kick #channel nick")?;
    let reason = (!reason.is_empty()).then_some(reason);
    Ok(vec![Message::new(
        "KICK",
        [channel, nick].into_iter().chain(reason),
    )])
}

fn invite(nick: String, channel: Option<String>) -> Result<Vec<Message>, String> {
    let channel = channel.ok_or("Use /invite in a channel, or name one: /invite nick #channel")?;
    Ok(vec![Message::new("INVITE", [nick, channel])])
}

/// `/mode +o bob` applies to the buffer's channel (or our nick elsewhere).
fn mode(channel: Option<String>, nick: &str, args: Vec<String>) -> Message {
    let implicit = args
        .first()
        .is_none_or(|first| first.starts_with(['+', '-']));
    let target = implicit.then(|| channel.unwrap_or_else(|| nick.to_string()));
    Message::new("MODE", target.into_iter().chain(args))
}

fn quit(reason: String, out: &mut Output) -> Vec<Message> {
    out.quit = true;
    let reason = (!reason.is_empty()).then_some(reason);
    vec![Message::new("QUIT", reason)]
}

fn query(
    session: &mut Session,
    nick: &str,
    text: Option<String>,
    out: &mut Output,
) -> Result<Vec<Message>, String> {
    out.event(Event::Joined {
        buffer: session.buffer(Target::Query(nick.to_string())),
    });
    open_query(session, nick, out);
    text.map_or(Ok(Vec::new()), |text| {
        send::to(session, nick, LineKind::Privmsg, &text, out)
    })
}

fn open_query(session: &mut Session, nick: &str, out: &mut Output) {
    let key = session.fold(nick);
    session.queries.insert(key);
    out.send.extend(history::resume(session, nick));
}

fn load_history(
    session: &mut Session,
    buffer: &BufferKey,
    before: Option<DateTime<Utc>>,
    limit: u16,
    out: &mut Output,
) {
    let name = target_name(&buffer.target).unwrap_or_default().to_string();
    let request = history::request(session, &name, before, limit);
    out.events.extend(history::unavailable(
        buffer,
        request.is_none(),
        session.reg.registered,
    ));
    out.send.extend(request);
}

/// Lines around one message; without the cap, an empty answer so the
/// owner stops waiting.
fn load_around(
    session: &mut Session,
    buffer: &BufferKey,
    point: (Option<&str>, DateTime<Utc>),
    limit: u16,
    out: &mut Output,
) {
    let name = target_name(&buffer.target).unwrap_or_default().to_string();
    match history::around(session, &name, point, limit) {
        Some(request) => out.send(request),
        None => out.event(Event::Backfill {
            buffer: buffer.clone(),
            lines: Vec::new(),
        }),
    }
}

fn target_name(target: &Target) -> Option<&str> {
    match target {
        Target::Channel(name) | Target::Query(name) => Some(name),
        Target::Server => None,
    }
}

fn mark_read(session: &Session, target: &Target, time: DateTime<Utc>) -> Option<Message> {
    let name = target_name(target).filter(|_| session.has_cap(READ_MARKER))?;
    let timestamp = format!("timestamp={}", lines::format_time(time));
    Some(Message::new("MARKREAD", [name.to_string(), timestamp]))
}

// qual:allow(dry, repeated_matches) reason: "This enum has one mapping of its own; rustqual groups unrelated enums only because their three-arm variant matches share one shape."
fn typing(session: &Session, target: &Target, state: Typing) -> Option<Message> {
    let name = target_name(target).filter(|_| session.has_cap(MESSAGE_TAGS))?;
    let value = match state {
        Typing::Active => "active",
        Typing::Paused => "paused",
        Typing::Done => "done",
    };
    Some(Message::new("TAGMSG", [name]).with_tag("+typing", value))
}
