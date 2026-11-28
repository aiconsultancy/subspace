//! Where the tab is offered, when a reply can be sent, and how long a
//! chosen thread lives.

use std::rc::Rc;

use subspace_client_irc::{ConnectionStatus, Event, LineKind};

use super::super::{MESSAGE_TAGS, NO_REPLY_TAG, NOT_CONNECTED, Threads, has_tab, reply_refusal};
use super::reply;
use crate::fixtures::{at, channel, line, query};
use crate::fleet::Roster;
use crate::fleet::tests::{agent, fleet_with};
use crate::state::LINE_CAP;
use crate::thread_status::tests::{joined, live, request, room, ship};

#[test]
fn the_tab_is_offered_in_verified_fleet_channels_only() {
    let aurora = fleet_with(
        "aurora",
        vec![
            agent("admiral-vance", "rearAdmiral", Some("aurora-admiral"), None),
            agent("aurora-lyra", "captain", Some("aurora-lyra"), Some("Lyra")),
        ],
        serde_json::json!({"starbaseChannel": "#aurora-starbase", "commandChannel": "#aurora-command"}),
    );
    let roster = Roster::new([&aurora]);
    assert!(has_tab(&roster, &channel("#aurora-lyra")));
    assert!(has_tab(&roster, &channel("#aurora-starbase")));
    assert!(has_tab(&roster, &channel("#aurora-command")));
    assert!(has_tab(&roster, &channel("#aurora-audit")));
    assert!(!has_tab(&roster, &channel("#lounge")));
    assert!(
        !has_tab(&roster, &query("aurora-lyra")),
        "never a private conversation"
    );
}

#[test]
fn a_reply_needs_a_connection_that_agreed_message_tags() {
    let mut state = joined(vec![ship()]);
    let network = || state.network(&crate::fixtures::network_id());
    assert_eq!(reply_refusal(None), Some(NOT_CONNECTED));
    assert_eq!(reply_refusal(network()), Some(NO_REPLY_TAG));
    state.apply(
        Event::Capabilities {
            network: crate::fixtures::network_id(),
            enabled: vec![MESSAGE_TAGS.into()],
        },
        at(1),
    );
    assert_eq!(
        reply_refusal(state.network(&crate::fixtures::network_id())),
        None
    );
    state.apply(
        Event::Status {
            network: crate::fixtures::network_id(),
            status: ConnectionStatus::Disconnected,
        },
        at(2),
    );
    assert_eq!(
        reply_refusal(state.network(&crate::fixtures::network_id())),
        Some(NOT_CONNECTED)
    );
}

#[test]
fn a_chosen_thread_is_rebuilt_only_when_its_buffer_changes() {
    let mut state = joined(vec![ship()]);
    live(&mut state, [request(&room(), "first", 1)]);
    let mut threads = Threads::default();
    threads.choose(&room(), "first".into());
    assert!(threads.refresh(&state).is_empty());
    let before = threads.shown(&room()).unwrap();
    assert!(threads.refresh(&state).is_empty());
    assert!(
        Rc::ptr_eq(&before, &threads.shown(&room()).unwrap()),
        "nothing changed, nothing read"
    );
    live(&mut state, [reply(&room(), "mira", "r1", "first", 2)]);
    threads.refresh(&state);
    assert_eq!(threads.shown(&room()).unwrap().replies.len(), 1, "live");
    let other = channel("#lounge");
    assert_eq!(threads.chosen(&other), None, "one choice per conversation");
}

#[test]
fn a_chosen_thread_goes_once_its_root_is_no_longer_held() {
    let mut state = joined(vec![ship()]);
    live(&mut state, [request(&room(), "first", 1)]);
    let mut threads = Threads::default();
    threads.choose(&room(), "first".into());
    threads.refresh(&state);
    let lines = (0..LINE_CAP).map(|n| {
        let minute = 2 + i64::try_from(n).unwrap_or_default();
        line(&room(), LineKind::Privmsg, "mira", "Busy morning.", minute)
    });
    live(&mut state, lines);
    assert_eq!(
        threads.refresh(&state),
        [(room(), "first".to_owned())],
        "the line cap dropped the root"
    );
    assert_eq!(threads.chosen(&room()), None);
    assert_eq!(threads.shown(&room()), None);
}

#[test]
fn a_chosen_thread_goes_with_its_conversation() {
    let mut state = joined(vec![ship()]);
    live(&mut state, [request(&room(), "first", 1)]);
    let mut threads = Threads::default();
    threads.choose(&room(), "first".into());
    threads.refresh(&state);
    state.remove(&room());
    assert_eq!(threads.refresh(&state).len(), 1);
    assert_eq!(threads.chosen(&room()), None);
}
