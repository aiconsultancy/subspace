//! Reactions: the exact `TAGMSG`s sent, refusals that send nothing, and
//! reactions read live, echoed and from history.

use super::{ALL_CAPS, PLAIN_CAPS, buffer, channel, command, config, feed, in_channel, now, sent};
use crate::model::{
    BufferKey, Command, Event, Reaction, ReactionChange, ReactionError, Target, Update,
};
use crate::session::{Carry, Output, Session};

fn react(buffer: BufferKey, msgid: &str, reaction: &str) -> Command {
    Command::React {
        buffer,
        msgid: msgid.into(),
        reaction: reaction.into(),
    }
}

fn unreact(buffer: BufferKey, msgid: &str, reaction: &str) -> Command {
    Command::Unreact {
        buffer,
        msgid: msgid.into(),
        reaction: reaction.into(),
    }
}

fn reactions(out: &Output) -> Vec<&Reaction> {
    out.updates
        .iter()
        .filter_map(|update| match update {
            Update::Reaction(reaction) => Some(reaction),
            _ => None,
        })
        .collect()
}

fn refusal(out: &Output) -> Option<&ReactionError> {
    match out.updates.as_slice() {
        [Update::ReactionRefused { error, .. }] => Some(error),
        _ => None,
    }
}

#[test]
fn reacting_and_unreacting_send_one_tagmsg_naming_the_message() {
    let mut session = in_channel(ALL_CAPS);
    let added = command(&mut session, react(channel("#c"), "m1", "👍"));
    assert_eq!(sent(&added), ["@+draft/react=👍;+draft/reply=m1 TAGMSG #c"]);
    assert!(added.updates.is_empty(), "echo-message returns it instead");
    let removed = command(&mut session, unreact(channel("#c"), "m1", "👍"));
    assert_eq!(
        sent(&removed),
        ["@+draft/reply=m1;+draft/unreact=👍 TAGMSG #c"]
    );
    let private = command(
        &mut session,
        react(buffer(Target::Query("bob".into())), "p1", "a;b c"),
    );
    assert_eq!(
        sent(&private),
        [r"@+draft/react=a\:b\sc;+draft/reply=p1 TAGMSG bob"],
        "values are escaped by the serialiser"
    );
}

#[test]
fn without_echo_message_our_reaction_is_echoed_locally() {
    let mut session = in_channel(PLAIN_CAPS);
    let out = command(&mut session, react(channel("#c"), "m1", "🛑"));
    assert_eq!(sent(&out).len(), 1);
    let echoed = reactions(&out);
    assert_eq!(echoed.len(), 1);
    let echo = echoed[0];
    assert!(echo.own);
    assert_eq!(echo.nick, "subspace");
    assert_eq!((echo.msgid.as_str(), echo.reaction.as_str()), ("m1", "🛑"));
    assert_eq!(echo.change, ReactionChange::Added);
    assert_eq!(echo.buffer, channel("#c"));
    assert!(!echo.history);
}

#[test]
fn reactions_are_refused_without_sending_when_they_cannot_go() {
    let cases: [(Command, ReactionError); 7] = [
        (react(channel("#c"), "", "👍"), ReactionError::NoMessageId),
        (
            react(channel("#c"), "m 1", "👍"),
            ReactionError::InvalidMessageId,
        ),
        (
            react(channel("#c"), &"m".repeat(129), "👍"),
            ReactionError::InvalidMessageId,
        ),
        (
            react(channel("#c"), "m1", ""),
            ReactionError::InvalidReaction,
        ),
        (
            react(channel("#c"), "m1", " 👍"),
            ReactionError::InvalidReaction,
        ),
        (
            react(channel("#c"), "m1", "a\u{1}b"),
            ReactionError::InvalidReaction,
        ),
        (
            react(buffer(Target::Server), "m1", "👍"),
            ReactionError::NoConversation,
        ),
    ];
    let mut session = in_channel(ALL_CAPS);
    for (request, error) in cases {
        let out = command(&mut session, request);
        assert!(out.send.is_empty());
        assert_eq!(refusal(&out), Some(&error));
    }
}

#[test]
fn reactions_need_message_tags_and_a_registered_connection() {
    let mut untagged = in_channel("batch server-time echo-message");
    let out = command(&mut untagged, unreact(channel("#c"), "m1", "👍"));
    assert!(out.send.is_empty());
    assert_eq!(refusal(&out), Some(&ReactionError::Unsupported));

    let mut registering = Session::new(config(), Carry::default());
    registering.start(now());
    let out = command(&mut registering, react(channel("#c"), "m1", "👍"));
    assert!(out.send.is_empty());
    assert_eq!(refusal(&out), Some(&ReactionError::NotConnected));
}

#[test]
fn live_reactions_and_our_echoes_become_updates() {
    let mut session = in_channel(ALL_CAPS);
    let out = feed(
        &mut session,
        &[
            "@time=2026-01-02T03:04:06.000Z;msgid=r1;account=alice;+draft/react=👍;+draft/reply=m1 :alice!a@h TAGMSG #c",
            "@account=*;+draft/unreact=👍;+draft/reply=m1 :bob!b@h TAGMSG #c",
            "@+draft/react=🎉;+draft/reply=m1 :subspace!u@h TAGMSG #c",
            "@+draft/react=👀;+reply=p1 :bob!b@h TAGMSG subspace",
        ],
    );
    let held = reactions(&out);
    assert_eq!(held.len(), 4);
    assert_eq!(held[0].nick, "alice");
    assert_eq!(held[0].account.as_deref(), Some("alice"));
    assert_eq!(held[0].id.as_deref(), Some("r1"));
    assert_eq!(held[0].time.to_rfc3339(), "2026-01-02T03:04:06+00:00");
    assert!(!held[0].own);
    assert_eq!(held[1].change, ReactionChange::Removed);
    assert_eq!(held[1].account, None, "`*` is no account");
    assert!(held[2].own, "our echo is ours");
    assert_eq!(held[3].buffer, buffer(Target::Query("bob".into())));
    assert_eq!(held[3].msgid, "p1", "the final reply tag is read too");
    assert!(out.events.is_empty(), "reactions are not lines");
}

#[test]
fn malformed_reactions_are_ignored_and_typing_still_reads() {
    let mut session = in_channel(ALL_CAPS);
    let long = "x".repeat(33);
    let out = feed(
        &mut session,
        &[
            "@+draft/react=👍;+draft/unreact=👍;+draft/reply=m1 :alice!a@h TAGMSG #c",
            "@+draft/react=👍 :alice!a@h TAGMSG #c",
            "@+draft/react=👍;+draft/reply :alice!a@h TAGMSG #c",
            &format!("@+draft/react={long};+draft/reply=m1 :alice!a@h TAGMSG #c"),
            r"@+draft/react=a\nb;+draft/reply=m1 :alice!a@h TAGMSG #c",
            "@+draft/react=👍;+draft/reply=m1 :alice!a@h PRIVMSG #c :not a TAGMSG",
            "@+draft/react=👍;+draft/reply=m1 :ergo.test TAGMSG #c",
            "@+typing=active :alice!a@h TAGMSG #c",
        ],
    );
    assert!(reactions(&out).is_empty());
    assert!(
        out.events
            .iter()
            .any(|event| matches!(event, Event::Typing { nick, .. } if nick == "alice"))
    );
}

#[test]
fn history_pages_report_their_reactions_after_the_page() {
    let mut session = in_channel(ALL_CAPS);
    let out = feed(
        &mut session,
        &[
            ":ergo.test BATCH +h draft/chathistory #c",
            "@batch=h;msgid=m1;time=2026-01-02T03:00:00.000Z :alice!a@h PRIVMSG #c :ship it",
            "@batch=h;msgid=r1;time=2026-01-02T03:01:00.000Z;account=bob;+draft/react=🚀;+draft/reply=m1 :bob!b@h TAGMSG #c",
            ":ergo.test BATCH -h",
        ],
    );
    let page = out.events.iter().find_map(|event| match event {
        Event::History { lines, .. } => Some(lines),
        _ => None,
    });
    assert_eq!(page.map(Vec::len), Some(1), "the reaction is not a line");
    let held = reactions(&out);
    assert_eq!(held.len(), 1);
    assert!(held[0].history);
    assert_eq!(held[0].buffer, channel("#c"));
    assert_eq!(held[0].account.as_deref(), Some("bob"));
}
