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

/// Sends the request's `TAGMSG`, or reports why nothing was sent. The local
/// echo (without `echo-message`) is reported only once the write succeeds;
/// a failed write answers [`ReactionError::ConnectionLost`] instead.
fn send(session: &Session, request: Request, out: &mut Output) {
    match prepare(session, &request) {
        Ok(message) => {
            out.send(message);
            out.updates
                .unwritten
                .push(refused(&request, ReactionError::ConnectionLost));
            out.updates.written.extend(local_echo(session, request));
        }
        Err(error) => out.updates.now.push(refused(&request, error)),
    }
}

fn refused(request: &Request, error: ReactionError) -> Update {
    Update::ReactionRefused {
        buffer: request.buffer.clone(),
        msgid: request.msgid.clone(),
        error,
    }
}

/// Whether this connection can send reactions: `message-tags` and no
/// `CLIENTTAGDENY` for either reaction tag or the reply tag they need.
pub(super) fn supported(session: &Session) -> bool {
    session.has_cap(reactions::CAPABILITY)
        && [REACT_TAG, UNREACT_TAG, REPLY]
            .iter()
            .all(|tag| session.isupport.allows_tag(tag.trim_start_matches('+')))
}

/// [`Update::ReactionSupport`] for this connection.
pub(super) fn support(session: &Session) -> Update {
    Update::ReactionSupport {
        network: session.network(),
        supported: supported(session),
    }
}

/// The `TAGMSG` for `request`, after every check.
fn prepare(session: &Session, request: &Request) -> Result<Message, ReactionError> {
    if !session.registered() {
        return Err(ReactionError::NotConnected);
    }
    if !supported(session) {
        return Err(ReactionError::Unsupported);
    }
    let target = destination(session, &request.buffer.target)?;
    let enabled = [reactions::CAPABILITY.to_owned()];
    reactions::check_reaction(&enabled, true, Some(&request.msgid), &request.reaction)?;
    let tag = match request.change {
        ReactionChange::Added => REACT_TAG,
        ReactionChange::Removed => UNREACT_TAG,
    };
    Ok(Message::new("TAGMSG", [target])
        .with_tag(tag, &request.reaction)
        .with_tag(REPLY, &request.msgid))
}

/// The buffer's one destination: a channel name the server calls a channel,
/// or a nick that is not one, without spaces, commas or controls.
fn destination<'a>(session: &Session, target: &'a Target) -> Result<&'a str, ReactionError> {
    let valid = |name: &str| {
        !name.is_empty()
            && !name.starts_with(':')
            && !name
                .chars()
                .any(|c| c == ',' || c.is_whitespace() || c.is_control())
    };
    match target {
        Target::Server => Err(ReactionError::NoConversation),
        Target::Channel(name) if valid(name) && session.isupport.channel(name) == Some(name) => {
            Ok(name)
        }
        Target::Query(nick) if valid(nick) && session.isupport.channel(nick).is_none() => Ok(nick),
        _ => Err(ReactionError::InvalidTarget),
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
            account: session.reg.account.clone(),
            own: true,
            time: session.now,
            id: None,
            history: false,
            casemapping: session.isupport.casemapping,
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
    let account = message
        .tag("account")
        .filter(|account| !account.is_empty() && *account != "*");
    Some(Reaction {
        buffer: session.buffer(target),
        msgid: msgid.to_owned(),
        reaction: reaction.to_owned(),
        change,
        nick: nick.to_owned(),
        account: account.map(str::to_owned),
        own: ours(session, nick, account),
        time: message
            .tag("time")
            .and_then(lines::parse_time)
            .unwrap_or(session.now),
        id: message.tag("msgid").map(str::to_owned),
        history,
        casemapping: session.isupport.casemapping,
    })
}

/// Ours: the same account when both are known, else our current nick.
fn ours(session: &Session, nick: &str, account: Option<&str>) -> bool {
    match (account, session.reg.account.as_deref()) {
        (Some(theirs), Some(mine)) => session.fold(theirs) == session.fold(mine),
        _ => session.is_own(nick),
    }
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
