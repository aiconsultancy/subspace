//! A ship row's one dot and a person's presence mark, as the layout
//! records them.

use subspace_client_irc::{ConnectionStatus, Event, Member};

use super::super::activity::spoken;
use super::super::{Activity, Presence, row_key};
use super::fleets::{aurora, joined};
use crate::fixtures::{at, channel, member, message, network_id, query};
use crate::state::IrcState;

fn signed(nick: &str) -> Member {
    let mut signed = member(nick, "");
    signed.account = Some(nick.to_owned());
    signed
}

/// Lyra's nick status, or none.
fn lyra_status(status: Option<&str>) -> Event {
    Event::NickStatus {
        network: network_id(),
        nick: "aurora-lyra".into(),
        status: status.map(str::to_owned),
    }
}

/// The ship channel's members: only `member`.
fn crew(member: Member) -> Event {
    Event::Members {
        buffer: channel("#aurora-lyra"),
        members: vec![member],
    }
}

/// The ship row's trailing marks and whether its dot says working.
fn note(state: &IrcState) -> (Vec<String>, Option<bool>) {
    let layout = super::super::layout(state, &aurora());
    let row = &layout.groups[0].rows[0];
    let marks = row.trailing.iter().map(|glyph| glyph.text.to_string());
    let working = layout.ships.get(&row.key).map(Activity::is_working);
    (marks.collect(), working)
}

#[test]
fn a_ship_s_dot_says_working_only_while_its_vouched_captain_has_a_status() {
    let ship = channel("#aurora-lyra");
    let mut state = joined([ship.clone()]);
    let connected = Event::Status {
        network: network_id(),
        status: ConnectionStatus::Connected { nick: "me".into() },
    };
    for (minute, event) in [
        connected,
        lyra_status(Some("Reviewing NAV-7")),
        crew(member("aurora-lyra", "")),
    ]
    .into_iter()
    .enumerate()
    {
        state.apply(event, at(minute as i64));
    }
    assert_eq!(
        note(&state),
        (vec![], Some(false)),
        "no account, not working"
    );
    state.apply(crew(signed("aurora-lyra")), at(4));
    assert_eq!(note(&state), (vec![], Some(true)));
    state.apply(Event::Line(message(&ship, "mira", "hi", 5)), at(5));
    assert_eq!(
        note(&state),
        (vec!["1".to_owned()], Some(true)),
        "the count stays text; the dot is apart"
    );
    state.apply(lyra_status(None), at(6));
    assert_eq!(note(&state), (vec!["1".to_owned()], Some(false)));
}

#[test]
fn the_dot_names_its_captain_and_its_tooltip_says_what_is_at_work() {
    let working = Activity {
        captain: "aurora-lyra".into(),
        working: vec!["Reviewing NAV-7".into()],
    };
    assert!(working.is_working());
    assert_eq!(working.label(), "aurora-lyra: working");
    assert_eq!(working.tooltip(), "Reviewing NAV-7");
    let idle = Activity {
        captain: "aurora-lyra".into(),
        working: Vec::new(),
    };
    assert!(!idle.is_working());
    assert_eq!(idle.label(), "aurora-lyra: idle");
    assert_eq!(idle.tooltip(), "aurora-lyra: idle");
    assert_eq!(
        spoken("lyra, 1 unread", Some(&working.label())),
        "lyra, 1 unread, aurora-lyra: working"
    );
    assert_eq!(spoken("lyra", None), "lyra");
}

#[test]
fn a_person_s_presence_comes_from_the_channels_we_share() {
    let mut state = joined([
        query("mira"),
        query("tavi"),
        query("anna"),
        channel("#lounge"),
    ]);
    let mut tavi = member("tavi", "");
    tavi.away = true;
    state.apply(
        Event::Members {
            buffer: channel("#lounge"),
            members: vec![member("Mira", ""), tavi],
        },
        at(1),
    );
    let layout = super::super::layout(&state, &aurora());
    let presence = |nick: &str| layout.people.get(&row_key(&query(nick))).copied();
    assert_eq!(presence("mira"), Some(Presence::Here), "ignoring case");
    assert_eq!(presence("tavi"), Some(Presence::Away));
    assert_eq!(
        presence("anna"),
        Some(Presence::Unseen),
        "nothing is guessed"
    );
    assert_eq!(Presence::Unseen.spoken(), "not in a shared channel");
    assert!(!layout.people.contains_key(&row_key(&channel("#lounge"))));
}
