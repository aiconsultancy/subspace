use super::{ALL_CAPS, PLAIN_CAPS, channel, feed, in_channel, lines, registered, sent};
use crate::model::{Event, LineKind, Member};

fn member(nick: &str, prefixes: &str) -> Member {
    Member {
        nick: nick.into(),
        prefixes: prefixes.into(),
        away: false,
        account: None,
    }
}

fn members(events: &[Event]) -> Option<Vec<Member>> {
    events.iter().rev().find_map(|event| match event {
        Event::Members { members, .. } => Some(members.clone()),
        _ => None,
    })
}

#[test]
fn own_join_opens_the_buffer_and_requests_history() {
    let mut session = registered(PLAIN_CAPS);
    let out = feed(&mut session, &[":subspace!u@h JOIN #c"]);
    assert!(out.events.contains(&Event::Joined {
        buffer: channel("#c")
    }));
    assert_eq!(sent(&out), ["CHATHISTORY LATEST #c * 100"]);
    assert_eq!(lines(&out)[0].kind, LineKind::Join);
    assert!(lines(&out)[0].own);
    assert_eq!(session.channel_names(), ["#c"]);
}

#[test]
fn names_build_a_sorted_member_list_and_ask_who() {
    let mut session = registered(ALL_CAPS);
    feed(&mut session, &[":subspace!u@h JOIN #c * :Subspace"]);
    let out = feed(
        &mut session,
        &[
            ":ergo.test 353 subspace = #c :+bob!b@h @+alice!a@h",
            ":ergo.test 353 subspace = #c :subspace!u@h",
            ":ergo.test 366 subspace #c :End of NAMES list",
        ],
    );
    assert_eq!(
        members(&out.events),
        Some(vec![
            member("alice", "@+"),
            member("bob", "+"),
            member("subspace", "")
        ])
    );
    assert_eq!(sent(&out), ["WHO #c %tnfa,42"]);

    let who = feed(
        &mut session,
        &[
            ":ergo.test 354 subspace 42 alice H alice",
            ":ergo.test 354 subspace 42 bob G 0",
            ":ergo.test 315 subspace #c :End of WHO list",
        ],
    );
    let list = members(&who.events).unwrap_or_default();
    assert_eq!(list[0].account.as_deref(), Some("alice"));
    assert!(list[1].away);
    assert_eq!(list[1].account, None);
}

#[test]
fn mode_changes_update_prefixes() {
    let mut session = in_channel(PLAIN_CAPS);
    let out = feed(&mut session, &[":alice!a@h MODE #c +o-v+l bob bob 10"]);
    assert!(out.events.contains(&Event::MemberChanged {
        buffer: channel("#c"),
        old_nick: "bob".into(),
        member: member("bob", "@"),
    }));
    assert_eq!(lines(&out)[0].text, "sets mode +o-v+l bob bob 10");
    let own_mode = feed(&mut session, &[":subspace MODE subspace +i"]);
    assert_eq!(
        lines(&own_mode)[0].buffer.target,
        crate::model::Target::Server
    );
}

#[test]
fn joins_parts_quits_and_kicks_track_members() {
    let mut session = in_channel(ALL_CAPS);
    let join = feed(&mut session, &[":carol!c@h JOIN #c carol :Carol"]);
    assert!(join.events.contains(&Event::MemberJoined {
        buffer: channel("#c"),
        member: Member {
            account: Some("carol".into()),
            ..member("carol", "")
        },
    }));
    let part = feed(&mut session, &[":carol!c@h PART #c :bye"]);
    assert_eq!(lines(&part)[0].text, "left (bye)");
    assert!(part.events.contains(&Event::MemberLeft {
        buffer: channel("#c"),
        nick: "carol".into(),
    }));
    let quit = feed(&mut session, &[":bob!b@h QUIT :Quit: gone"]);
    assert_eq!(lines(&quit)[0].kind, LineKind::Quit);
    assert_eq!(lines(&quit)[0].text, "quit (Quit: gone)");
    let kick = feed(&mut session, &[":alice!a@h KICK #c subspace :out"]);
    assert_eq!(lines(&kick)[0].text, "kicked subspace (out)");
    assert!(kick.events.contains(&Event::Left {
        buffer: channel("#c")
    }));
    assert!(session.channel_names().is_empty());
}

#[test]
fn nick_account_and_away_changes_update_members() {
    let mut session = in_channel(ALL_CAPS);
    let nick = feed(&mut session, &[":bob!b@h NICK robert"]);
    assert!(nick.events.contains(&Event::MemberChanged {
        buffer: channel("#c"),
        old_nick: "bob".into(),
        member: member("robert", "+"),
    }));
    assert_eq!(lines(&nick)[0].text, "is now known as robert");
    let account = feed(&mut session, &[":robert!b@h ACCOUNT robert"]);
    assert!(matches!(
        &account.events[0],
        Event::MemberChanged { member, .. } if member.account.as_deref() == Some("robert")
    ));
    let away = feed(&mut session, &[":robert!b@h AWAY :lunch"]);
    assert!(matches!(&away.events[0], Event::MemberChanged { member, .. } if member.away));
    let own = feed(&mut session, &[":subspace!u@h NICK newme"]);
    assert!(own.events.iter().any(|event| matches!(
        event,
        Event::OwnNick { nick, .. } if nick == "newme"
    )));
}

#[test]
fn topics_and_invites() {
    let mut session = in_channel(PLAIN_CAPS);
    let reply = feed(
        &mut session,
        &[
            ":ergo.test 332 subspace #c :Welcome to #c",
            ":ergo.test 333 subspace #c alice!a@h 1700000000",
        ],
    );
    assert!(reply.events.contains(&Event::Topic {
        buffer: channel("#c"),
        topic: "Welcome to #c".into(),
        set_by: Some("alice".into()),
    }));
    let change = feed(&mut session, &[":alice!a@h TOPIC #c :New topic"]);
    assert_eq!(lines(&change)[0].text, "changed the topic to: New topic");
    let invite = feed(&mut session, &[":alice!a@h INVITE subspace #other"]);
    assert_eq!(
        lines(&invite)[0].buffer.target,
        crate::model::Target::Server
    );
    assert_eq!(lines(&invite)[0].text, "invited subspace to #other");
}

#[test]
fn channel_errors_go_to_the_channel() {
    let mut session = in_channel(PLAIN_CAPS);
    let out = feed(
        &mut session,
        &[
            ":ergo.test 482 subspace #c :You're not a channel operator",
            ":ergo.test 401 subspace nobody :No such nick",
            ":ergo.test 372 subspace :- MOTD text",
        ],
    );
    let lines = lines(&out);
    assert_eq!(lines[0].buffer, channel("#c"));
    assert_eq!(lines[0].kind, LineKind::Error);
    assert_eq!(lines[1].buffer.target, crate::model::Target::Server);
    assert_eq!(lines[2].kind, LineKind::Info);
    assert_eq!(lines[2].text, "- MOTD text");
}

#[test]
fn an_at_tag_of_our_current_nick_is_a_mention() {
    let mut session = in_channel(ALL_CAPS);
    let out = feed(
        &mut session,
        &[
            ":alice!a@h PRIVMSG #c :@subspace can you review?",
            ":subspace!u@h PRIVMSG #c :@subspace note to self",
            ":subspace!u@h NICK :captain",
            ":alice!a@h PRIVMSG #c :@subspace still there?",
            ":alice!a@h PRIVMSG #c :@captain now?",
        ],
    );
    let mentions: Vec<(bool, bool)> = lines(&out)
        .into_iter()
        .filter(|line| line.kind == LineKind::Privmsg)
        .map(|line| (line.highlight, line.own))
        .collect();
    assert_eq!(
        mentions,
        [(true, false), (false, true), (false, false), (true, false)],
        "a tag of the old nick no longer mentions us; our own line never does"
    );
}
