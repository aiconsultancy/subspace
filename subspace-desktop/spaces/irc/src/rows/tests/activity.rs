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
    let layout = super::super::layout(state, &aurora(), &Default::default());
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
        crew: None,
    };
    assert!(working.is_working());
    assert_eq!(working.label(), "aurora-lyra: working");
    assert_eq!(working.tooltip(), "Reviewing NAV-7");
    let idle = Activity {
        working: Vec::new(),
        ..working.clone()
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
    let layout = super::super::layout(&state, &aurora(), &Default::default());
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

/// The aurora fleet's word, as the hub holds it, in `state`.
fn lives(state: subspace_client_fleet::FleetState) -> crate::live::Lives {
    [("aurora".to_owned(), crate::live::tests::aurora(state))].into()
}

/// Lyra's row's dot with `lives`.
fn lyra_dot(state: &IrcState, lives: &crate::live::Lives) -> Option<Activity> {
    let layout = super::super::layout(state, &aurora(), lives);
    layout
        .ships
        .get(&row_key(&channel("#aurora-lyra")))
        .cloned()
}

#[test]
fn with_live_data_a_ship_s_dot_follows_its_crew_and_names_who_works_on_what() {
    use subspace_client_fleet::{FleetState, Link};
    let mut state = joined([channel("#aurora-lyra"), channel("#aurora-orion")]);
    // The captain's own status says nothing once the crew is known.
    state.apply(lyra_status(Some("Reviewing NAV-7")), at(1));
    state.apply(crew(signed("aurora-lyra")), at(2));
    let live = lives(FleetState::Followed(Link::Live));
    let dot = lyra_dot(&state, &live).unwrap();
    assert!(dot.is_working());
    assert_eq!(dot.label(), "aurora-lyra: 3 crew working");
    assert_eq!(
        dot.tooltip(),
        "3 crew working\n\
         Wren · Cancel tile decodes that leave the viewport (NAV-7)\n\
         Quill · Tab from a chart marker to the burn summary (NAV-11)\n\
         Juniper · Course correction preview (NAV-2)"
    );
    // Orion's detail has not come: its totals still say one is working.
    let layout = super::super::layout(&state, &aurora(), &live);
    let orion = layout
        .ships
        .get(&row_key(&channel("#aurora-orion")))
        .unwrap();
    assert_eq!(
        (orion.is_working(), orion.tooltip().as_str()),
        (true, "1 crew working")
    );
    // Not live: the captain's vouched status, as before.
    let down = lives(FleetState::Followed(Link::Unreachable));
    let dot = lyra_dot(&state, &down).unwrap();
    assert_eq!(
        (dot.is_working(), dot.tooltip().as_str()),
        (true, "Reviewing NAV-7")
    );
    assert_eq!(dot.label(), "aurora-lyra: working");
    let dot = lyra_dot(&state, &Default::default()).unwrap();
    assert_eq!(dot.crew, None, "no fleet followed");
}

#[test]
fn with_live_data_and_no_crew_at_work_the_dot_is_a_ring_whatever_the_captain_says() {
    let ship = crate::live::ShipLive {
        crew: Some((0, 4)),
        members: Vec::new(),
        ..crate::live::tests::lyra()
    };
    let statuses = {
        let mut statuses = crate::state::Statuses::default();
        statuses.set("aurora-lyra", Some("Reviewing NAV-7"));
        statuses
    };
    let members = [signed("aurora-lyra")];
    let dot = super::super::activity::ship("aurora-lyra", &statuses, &members, Some(&ship));
    assert!(!dot.is_working());
    assert_eq!(dot.label(), "aurora-lyra: no crew working");
    assert_eq!(dot.tooltip(), "No crew working");
    // A ship whose crew the API does not count falls back to the captain.
    let uncounted = crate::live::ShipLive { crew: None, ..ship };
    let dot = super::super::activity::ship("aurora-lyra", &statuses, &members, Some(&uncounted));
    assert_eq!((dot.crew, dot.is_working()), (None, true));
}
