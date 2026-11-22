//! Reactions: `TAGMSG` with `+draft/react` or `+draft/unreact` and
//! `+draft/reply`, sent for owners and read live, echoed and from history.

use super::{Output, Session, lines, send::REPLY};
use crate::model::{BufferKey, Command, Reaction, ReactionChange, ReactionError, Target, Update};
use crate::proto::Message;
use crate::reactions::{self, REACT_TAG, UNREACT_TAG};

const ECHO: &str = "echo-message";
/// The final, unprefixed reply tag, read as a fallback as Stargate does.
const FINAL_REPLY: &str = "+reply";

/// What an owner asked: add or take back `reaction` on `msgid`.
struct Request {
    buffer: BufferKey,
    msgid: String,
    reaction: String,
    change: ReactionChange,
}

/// Answers [`Command::React`] and [`Command::Unreact`]; ignores the rest.
pub(super) fn command(session: &Session, command: Command, out: &mut Output) {
    if let Some(request) = request(command) {
        send(session, request, out);
    }
}

fn request(command: Command) -> Option<Request> {
    let (buffer, msgid, reaction, change) = match command {
        Command::React {
            buffer,
            msgid,
            reaction,
        } => (buffer, msgid, reaction, ReactionChange::Added),
        Command::Unreact {
            buffer,
            msgid,
            reaction,
        } => (buffer, msgid, reaction, ReactionChange::Removed),
        _ => return None,
    };
    Some(Request {
        buffer,
        msgid,
        reaction,
        change,
    })
}

/// Sends the request's `TAGMSG`, with a local echo when the server will not
/// echo it, or reports why nothing was sent.
fn send(session: &Session, request: Request, out: &mut Output) {
    match prepare(session, &request) {
        Ok(message) => {
            out.send(message);
            out.updates.extend(local_echo(session, request));
        }
        Err(error) => out.updates.push(Update::ReactionRefused {
            buffer: request.buffer,
            msgid: request.msgid,
            error,
        }),
    }
}

/// The `TAGMSG` for `request`, after every check.
fn prepare(session: &Session, request: &Request) -> Result<Message, ReactionError> {
    let target = conversation(&request.buffer.target);
    let enabled: Vec<String> = session.caps.enabled.iter().cloned().collect();
    if !session.registered() {
        return Err(ReactionError::NotConnected);
    }
    reactions::check_reaction(
        &enabled,
        target.is_some(),
        Some(&request.msgid),
        &request.reaction,
    )?;
    let tag = match request.change {
        ReactionChange::Added => REACT_TAG,
        ReactionChange::Removed => UNREACT_TAG,
    };
    Ok(Message::new("TAGMSG", [target.unwrap_or_default()])
        .with_tag(tag, &request.reaction)
        .with_tag(REPLY, &request.msgid))
}

fn conversation(target: &Target) -> Option<&str> {
    match target {
        Target::Channel(name) | Target::Query(name) => Some(name),
        Target::Server => None,
    }
}

/// Our reaction as we sent it, when `echo-message` will not return it.
fn local_echo(session: &Session, request: Request) -> Option<Update> {
    (!session.has_cap(ECHO)).then(|| {
        Update::Reaction(Reaction {
            buffer: request.buffer,
            msgid: request.msgid,
            reaction: request.reaction,
            change: request.change,
            nick: session.reg.nick.clone(),
            account: None,
            own: true,
            time: session.now,
            id: None,
            history: false,
        })
    })
}

/// A live or echoed reaction in `message`, if it carries a valid one.
pub(super) fn live(session: &Session, message: &Message) -> Option<Update> {
    let target = lines::message_target(session, message);
    parse(session, message, target, false).map(Update::Reaction)
}

/// The reactions among a history page's messages, in page order.
pub(super) fn history(session: &Session, messages: &[Message], target: &Target) -> Vec<Update> {
    messages
        .iter()
        .filter_map(|message| parse(session, message, target.clone(), true))
        .map(Update::Reaction)
        .collect()
}

/// A reaction from untrusted input: a `TAGMSG` from a nick with exactly one
/// of the two reaction tags, a valid reply `msgid` and a valid reaction.
fn parse(session: &Session, message: &Message, target: Target, history: bool) -> Option<Reaction> {
    let (reaction, change) = reaction_tag(message)?;
    let msgid = message.tag(REPLY).or_else(|| message.tag(FINAL_REPLY));
    let msgid = reactions::validate_msgid(msgid).ok()?;
    let reaction = reactions::validate_reaction(reaction).ok()?;
    let nick = message.nick()?;
    Some(Reaction {
        buffer: session.buffer(target),
        msgid: msgid.to_owned(),
        reaction: reaction.to_owned(),
        change,
        nick: nick.to_owned(),
        account: message
            .tag("account")
            .filter(|account| !account.is_empty() && *account != "*")
            .map(str::to_owned),
        own: session.is_own(nick),
        time: message
            .tag("time")
            .and_then(lines::parse_time)
            .unwrap_or(session.now),
        id: message.tag("msgid").map(str::to_owned),
        history,
    })
}

/// The reaction and whether it was added; `None` unless a `TAGMSG` has
/// exactly one of the two tags (the draft forbids both on one message).
fn reaction_tag(message: &Message) -> Option<(&str, ReactionChange)> {
    if message.command != "TAGMSG" {
        return None;
    }
    match (message.tag(REACT_TAG), message.tag(UNREACT_TAG)) {
        (Some(reaction), None) => Some((reaction, ReactionChange::Added)),
        (None, Some(reaction)) => Some((reaction, ReactionChange::Removed)),
        _ => None,
    }
}
