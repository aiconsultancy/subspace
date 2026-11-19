use super::super::{config, feed, in_channel, registered};
use super::{CAPS, statuses};
use crate::session::{Output, Session};

#[test]
fn quit_and_nick_clear_even_without_known_channel_membership() {
    for departure in [":ALICE!u@h QUIT :Bye", ":ALICE!u@h NICK bob"] {
        let mut session = registered(CAPS);
        feed(&mut session, &[":s 761 * Alice status * :Thinking"]);
        assert_eq!(
            statuses(&feed(&mut session, &[departure])),
            [("Alice", None)]
        );
        assert!(session.statuses.values.is_empty());
    }
}

#[test]
fn last_shared_channel_part_or_kick_clears_both_peers_and_our_departures() {
    for departure in [
        ":alice!u@h PART #c",
        ":op!u@h KICK #c alice :Bye",
        ":subspace!u@h PART #c",
        ":op!u@h KICK #c subspace :Bye",
    ] {
        let mut session = in_channel(CAPS);
        feed(&mut session, &[":s 761 * alice status * :Thinking"]);
        assert_eq!(
            statuses(&feed(&mut session, &[departure])),
            [("alice", None)]
        );
    }
}

#[test]
fn another_shared_channel_keeps_the_value_until_the_last_departure() {
    let mut session = in_channel(CAPS);
    feed(
        &mut session,
        &[
            ":subspace!u@h JOIN #d",
            ":alice!u@h JOIN #d",
            ":s 761 * alice status * :Thinking",
        ],
    );
    assert!(statuses(&feed(&mut session, &[":alice!u@h PART #c"])).is_empty());
    assert_eq!(
        statuses(&feed(&mut session, &[":alice!u@h PART #d"])),
        [("alice", None)]
    );
}

#[test]
fn unrelated_departures_do_not_clear_values_outside_the_member_maps() {
    let mut session = in_channel(CAPS);
    feed(&mut session, &[":s 761 * monitored status * :Thinking"]);
    assert!(statuses(&feed(&mut session, &[":alice!u@h PART #c"])).is_empty());
    assert_eq!(
        statuses(&feed(&mut session, &[":monitored!u@h QUIT :Bye"])),
        [("monitored", None)]
    );
}

#[test]
fn network_status_values_are_independent() {
    let mut a = registered(CAPS);
    let mut b = registered(CAPS);
    feed(&mut a, &[":s 761 * alice status * :Thinking"]);
    assert!(
        feed(&mut b, &[":s 766 * alice status :Deleted"])
            .events
            .is_empty()
    );
    assert_eq!(
        statuses(&feed(&mut a, &[":s 766 * alice status :Deleted"])),
        [("alice", None)]
    );
}

#[test]
fn disconnect_clears_all_values_once_and_carry_does_not_keep_them() {
    let mut session = registered(CAPS);
    feed(
        &mut session,
        &[
            ":s 761 * alice status * :Thinking",
            ":s 761 * bob status * :Running",
        ],
    );
    let events = session.abandon();
    let out = Output {
        events,
        ..Output::default()
    };
    assert_eq!(statuses(&out), [("alice", None), ("bob", None)]);
    assert!(session.abandon().is_empty());
    let fresh = Session::new(config(), session.into_carry());
    assert!(fresh.statuses.values.is_empty());
}

#[test]
fn rfc1459_and_changed_case_mapping_use_the_connections_rules() {
    let mut session = registered(CAPS);
    feed(
        &mut session,
        &[
            ":s 005 subspace CASEMAPPING=rfc1459 :Supported",
            ":s 761 * Agent[1] status * :Thinking",
        ],
    );
    assert!(
        statuses(&feed(
            &mut session,
            &[":s 761 * agent{1} status * :Thinking"]
        ))
        .is_empty()
    );
    assert_eq!(
        statuses(&feed(&mut session, &[":agent{1}!u@h QUIT :Bye"])),
        [("Agent[1]", None)]
    );
    feed(
        &mut session,
        &[
            ":s 005 subspace CASEMAPPING=ascii :Supported",
            ":s 761 * Agent[1] status * :Thinking",
            ":s 005 subspace CASEMAPPING=rfc1459 :Supported",
        ],
    );
    assert_eq!(
        statuses(&feed(&mut session, &[":s 766 * agent{1} status :Deleted"])),
        [("Agent[1]", None)]
    );
}
