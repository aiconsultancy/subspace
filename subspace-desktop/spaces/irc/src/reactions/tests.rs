//! The hub's reactions, the stop group's conditions and the choices.

use subspace_client_irc::reactions::ReactionSummary;
use subspace_client_irc::test_support;
use subspace_client_irc::{BufferKey, Event, ReactionError, Target, Update};

use super::choices::choices;
use super::stop::{AgentMessage, Sources, Stops, stops};
use super::{Reactions, refusal_text};
use crate::fixtures::{channel, message, network_id};
use crate::fleet::Roster;
use crate::fleet::tests::{agent as roster_agent, fleet};
use crate::state::IrcState;
use crate::thread_status::statuses;
use crate::thread_status::tests::{joined, live, progress, request, room, said, ship};

fn reaction(buffer: &BufferKey, msgid: &str, value: &str, nick: &str) -> Update {
    let mut reaction = test_support::reaction(buffer.clone(), msgid, value, nick);
    reaction.own = nick == "subspace";
    Update::Reaction(reaction)
}

fn support(supported: bool) -> Update {
    Update::ReactionSupport {
        network: network_id(),
        supported,
    }
}

/// `#ops` holding our request `r1` to astra and an ordinary message `m1`.
fn ops() -> IrcState {
    let mut state = joined(vec![ship()]);
    let mut ordinary = message(&room(), "carol", "lunch?", 1);
    ordinary.id = Some("m1".into());
    live(&mut state, [request(&room(), "r1", 2), ordinary]);
    state
}

/// Whether `state` holds the message a reaction names.
fn held_in(state: &IrcState, update: &Update) -> bool {
    match update {
        Update::Reaction(reaction) => state.buffer(&reaction.buffer).is_some_and(|held| {
            held.entries
                .iter()
                .any(|entry| entry.line.id.as_deref() == Some(&reaction.msgid))
        }),
        _ => false,
    }
}

fn shown(reactions: &Reactions, state: &IrcState) -> Vec<(String, usize, bool)> {
    let held = state.buffer(&room()).expect("held");
    reactions
        .shown(&room(), held)
        .values()
        .flatten()
        .map(|summary| (summary.reaction.clone(), summary.count(), summary.mine()))
        .collect()
}

#[test]
fn reactions_show_on_held_messages_and_go_with_their_conversation() {
    let state = ops();
    let mut reactions = Reactions::default();
    for update in [
        reaction(&room(), "m1", "👍", "carol"),
        reaction(&room(), "m1", "👍", "subspace"),
        reaction(&room(), "m1", "🎉", "dave"),
        reaction(&room(), "unloaded", "👀", "dave"),
    ] {
        let held = held_in(&state, &update);
        assert!(reactions.update(update, held));
    }
    assert_eq!(
        shown(&reactions, &state),
        [("👍".into(), 2, true), ("🎉".into(), 1, false)],
        "in the book's order; a message not held shows nothing"
    );
    reactions.forget(&room());
    assert!(shown(&reactions, &state).is_empty());
}

#[test]
fn support_follows_the_client_and_keeps_known_reactions() {
    let state = ops();
    let mut reactions = Reactions::default();
    assert!(!reactions.supported(&network_id()), "off until reported");
    assert!(reactions.update(support(true), false));
    assert!(!reactions.update(support(true), false), "no change");
    reactions.update(reaction(&room(), "m1", "👍", "carol"), false);
    assert!(reactions.update(support(false), false));
    assert!(!reactions.supported(&network_id()));
    assert_eq!(shown(&reactions, &state).len(), 1, "known reactions stay");
}

#[test]
fn refusals_are_kept_per_conversation_with_a_serial() {
    let mut reactions = Reactions::default();
    let refused = |error| Update::ReactionRefused {
        buffer: room(),
        msgid: "m1".into(),
        error,
    };
    reactions.update(refused(ReactionError::NotConnected), false);
    reactions.update(refused(ReactionError::ConnectionLost), false);
    let refusal = reactions.refusal(&room()).expect("refused");
    assert_eq!(refusal.serial, 2);
    assert_eq!(refusal.error, ReactionError::ConnectionLost);
    assert!(reactions.refusal(&channel("#other")).is_none());
    assert_eq!(
        refusal_text(&ReactionError::NotConnected),
        "Reaction not sent: not connected."
    );
}

#[test]
fn nick_changes_seen_as_events_follow_reactions() {
    let state = ops();
    let mut reactions = Reactions::default();
    reactions.update(reaction(&room(), "m1", "👍", "carol"), false);
    let renamed = crate::fixtures::member("caz", "");
    reactions.observe(&Event::MemberChanged {
        buffer: room(),
        old_nick: "carol".into(),
        member: renamed,
    });
    let held = state.buffer(&room()).expect("held");
    let summary: Vec<ReactionSummary> = reactions
        .shown(&room(), held)
        .into_values()
        .flatten()
        .collect();
    assert_eq!(summary[0].reactors[0].nick, "caz");
}

/// A roster whose fleet has the agent `astra`.
fn astra() -> Roster {
    let aurora = fleet(
        "aurora",
        vec![roster_agent(
            "astra",
            "captain",
            Some("astra"),
            Some("Astra"),
        )],
    );
    Roster::new([&aurora])
}

/// The stops `buffer` shows with `roster`, its status lines current
/// unless `stale`.
fn found(state: &IrcState, buffer: &BufferKey, roster: &Roster, stale: bool) -> Stops {
    let held = state.buffer(&room()).expect("held");
    let connected = (!stale).then_some(&(0..0));
    stops(&Sources {
        buffer,
        held,
        statuses: &statuses(held, connected),
        roster,
    })
}

fn agent(state: &IrcState, buffer: &BufferKey, id: &str) -> Option<AgentMessage> {
    let seq = state
        .buffer(&room())
        .and_then(|held| {
            held.entries
                .iter()
                .find(|entry| entry.line.id.as_deref() == Some(id))
        })
        .expect("entry")
        .seq;
    found(state, buffer, &astra(), false).remove(&seq)
}

#[test]
fn the_stop_group_needs_a_running_request_to_a_verified_agent() {
    let mut state = ops();
    assert_eq!(agent(&state, &room(), "r1"), None, "no trusted status yet");
    live(&mut state, [progress(&room(), "r1", "tool", 3)]);
    let running = Some(AgentMessage {
        ship: "astra".into(),
        running: true,
    });
    assert_eq!(agent(&state, &room(), "r1"), running, "the request itself");
    assert_eq!(agent(&state, &room(), "m1"), None, "an ordinary message");
    live(&mut state, [said(&room(), Some("r1"), "a1", 4)]);
    assert_eq!(agent(&state, &room(), "a1"), running, "the agent's reply");
    let mut impostor = said(&room(), Some("r1"), "x1", 5);
    impostor.account = None;
    live(&mut state, [impostor]);
    assert_eq!(
        agent(&state, &room(), "x1"),
        None,
        "an unverified nick is not the agent"
    );
    live(&mut state, [progress(&room(), "r1", "completed", 6)]);
    assert_eq!(
        agent(&state, &room(), "r1").map(|agent| agent.running),
        Some(false),
        "settled: an agent message, but no stop group"
    );
}

#[test]
fn a_stale_status_offers_no_stop() {
    let mut state = ops();
    live(&mut state, [progress(&room(), "r1", "tool", 3)]);
    let found = found(&state, &room(), &astra(), true);
    assert!(
        found.values().all(|agent| !agent.running),
        "not connected: stale"
    );
}

#[test]
fn private_conversations_never_offer_a_stop() {
    let query = BufferKey {
        network: network_id(),
        target: Target::Query("astra".into()),
    };
    let mut state = ops();
    live(&mut state, [progress(&room(), "r1", "tool", 3)]);
    assert_eq!(agent(&state, &query, "r1"), None);
}

#[test]
fn a_nick_not_on_the_roster_is_never_offered_a_stop() {
    let mut state = ops();
    live(&mut state, [progress(&room(), "r1", "tool", 3)]);
    let nobody = Roster::default();
    assert!(
        found(&state, &room(), &nobody, false).is_empty(),
        "a person posting an agent-style marker gets no Stop group"
    );
    assert!(!found(&state, &room(), &astra(), false).is_empty());
}

#[test]
fn choices_offer_the_common_set_and_the_stop_group_only_when_asked() {
    let summaries = [ReactionSummary {
        reaction: "\u{270B}".into(),
        reactors: Vec::new(),
        stops_agent: true,
    }];
    let plain = choices(&summaries, None);
    assert_eq!(plain.common.len(), 6);
    assert!(plain.stop.is_none());
    assert!(
        plain
            .common
            .iter()
            .all(|choice| choice.reaction != "\u{270B}"),
        "stop reactions are never among the common choices"
    );
    let stop = choices(&summaries, Some("astra")).stop.expect("stop group");
    assert_eq!(stop.label(), "Stop astra");
    let labels: Vec<String> = stop.choices.iter().map(|choice| choice.label()).collect();
    assert_eq!(labels, ["\u{270B}  Raised hand", "\u{1F6D1}  Stop sign"]);
}

#[test]
fn a_ticked_choice_takes_back_our_exact_values() {
    use subspace_client_irc::reactions::{ReactionSummary, Reactor};
    let ours = |reaction: &str| ReactionSummary {
        reaction: reaction.to_owned(),
        reactors: vec![Reactor {
            nick: "subspace".into(),
            account: None,
            own: true,
            time: crate::fixtures::at(1),
        }],
        stops_agent: false,
    };
    let summaries = [ours("\u{2764}"), ours("\u{2764}\u{FE0F}")];
    let heart = choices(&summaries, None).common[1].clone();
    assert_eq!(heart.name, "Heart");
    assert_eq!(
        heart.sends(),
        [
            ("\u{2764}".to_owned(), false),
            ("\u{2764}\u{FE0F}".to_owned(), false)
        ],
        "each variant we made goes"
    );
    let thumbs = choices(&summaries, None).common[0].clone();
    assert_eq!(thumbs.sends(), [("\u{1F44D}".to_owned(), true)]);
}
