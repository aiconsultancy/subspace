use super::{config, feed, now, registered, sent};
use crate::model::{Event, NetworkId};
use crate::session::{Carry, Output, Session};

const CAPS: &str = "batch draft/metadata-2";

mod lifecycle;

fn statuses(out: &Output) -> Vec<(&str, Option<&str>)> {
    out.events
        .iter()
        .filter_map(|event| match event {
            Event::NickStatus {
                network,
                nick,
                status,
            } => {
                assert_eq!(*network, NetworkId("local".into()));
                Some((nick.as_str(), status.as_deref()))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn metadata_is_requested_and_subscribed_only_after_registration() {
    let mut session = Session::new(config(), Carry::default());
    session.start(now());
    let ls = feed(
        &mut session,
        &[":s CAP * LS :batch draft/metadata-2=max-subs=100"],
    );
    assert_eq!(sent(&ls), ["CAP REQ :batch draft/metadata-2"]);
    assert_eq!(
        sent(&feed(
            &mut session,
            &[":s CAP * ACK :batch draft/metadata-2"]
        )),
        ["CAP END"]
    );
    let welcome = feed(&mut session, &[":s 001 subspace :Welcome"]);
    assert_eq!(sent(&welcome), ["METADATA * SUB status", "JOIN #subspace"]);
    assert!(
        feed(&mut session, &[":s CAP subspace ACK draft/metadata-2"])
            .send
            .is_empty()
    );
}

#[test]
fn absent_rejected_or_dependency_missing_metadata_sends_nothing() {
    for (offered, reply) in [
        ("batch", "ACK batch"),
        (CAPS, "NAK :batch draft/metadata-2"),
        ("draft/metadata-2", "NAK draft/metadata-2"),
    ] {
        let mut session = Session::new(config(), Carry::default());
        session.start(now());
        let ls = feed(&mut session, &[&format!(":s CAP * LS :{offered}")]);
        if offered == "draft/metadata-2" {
            assert_eq!(sent(&ls), ["CAP END"]);
        }
        let out = feed(
            &mut session,
            &[&format!(":s CAP * {reply}"), ":s 001 subspace :Welcome"],
        );
        assert!(!sent(&out).iter().any(|line| line.starts_with("METADATA")));
    }
}

#[test]
fn cap_notify_subscribes_after_a_late_ack_and_clears_on_removal() {
    let mut session = registered("batch cap-notify");
    assert_eq!(
        sent(&feed(
            &mut session,
            &[":s CAP subspace NEW draft/metadata-2"]
        )),
        ["CAP REQ draft/metadata-2"]
    );
    assert_eq!(
        sent(&feed(
            &mut session,
            &[":s CAP subspace ACK draft/metadata-2"]
        )),
        ["METADATA * SUB status"]
    );
    feed(&mut session, &[":s 761 * alice status * :Thinking"]);
    assert_eq!(
        statuses(&feed(
            &mut session,
            &[":s CAP subspace DEL draft/metadata-2"]
        )),
        [("alice", None)]
    );
}

#[test]
fn numeric_values_changes_and_deletions_are_deduplicated_by_case_mapping() {
    let mut session = registered(CAPS);
    let out = feed(
        &mut session,
        &[
            ":s 761 * Alice status * :Thinking",
            ":s 761 subspace ALICE status * :Thinking",
            ":s 761 * alice status * :Running",
            ":s 766 * ALIce status :Key deleted",
            ":s 766 * alice status :Key deleted",
        ],
    );
    assert_eq!(
        statuses(&out),
        [
            ("Alice", Some("Thinking")),
            ("Alice", Some("Running")),
            ("Alice", None)
        ]
    );
}

#[test]
fn draft_notifications_and_cleaned_empty_values_clear() {
    let mut session = registered(CAPS);
    let out = feed(
        &mut session,
        &[
            ":s METADATA alice status * :\u{2}Thinking\u{f}",
            ":s METADATA alice status *",
            ":s METADATA alice status * :Again",
            ":s METADATA alice status * :\u{2}\u{3}04 \u{f}",
        ],
    );
    assert_eq!(
        statuses(&out),
        [
            ("alice", Some("Thinking")),
            ("alice", None),
            ("alice", Some("Again")),
            ("alice", None)
        ]
    );
}

#[test]
fn metadata_batches_stream_each_targets_values_without_duplicate_events() {
    let mut session = registered(CAPS);
    assert!(
        feed(&mut session, &[":s BATCH +m metadata #c"])
            .events
            .is_empty()
    );
    let out = feed(
        &mut session,
        &[
            "@batch=m :s 761 * alice status * :Thinking",
            "PING :still-live",
            "@batch=m :s METADATA bob status * :Running",
            "@batch=m :s 761 * #c status * :Channel value",
        ],
    );
    assert_eq!(
        statuses(&out),
        [("alice", Some("Thinking")), ("bob", Some("Running"))]
    );
    assert_eq!(sent(&out), ["PONG still-live"]);
    assert!(feed(&mut session, &[":s BATCH -m"]).events.is_empty());
}

#[test]
fn replies_and_errors_are_quiet_and_sync_later_fetches_a_snapshot() {
    let mut session = registered(CAPS);
    let out = feed(
        &mut session,
        &[
            ":s 770 subspace status",
            ":s 771 subspace other",
            ":s 772 subspace status",
            ":s 773 subspace status :Too many subscriptions",
            ":s FAIL METADATA FORBIDDEN status :Disabled",
            ":s 764 subspace status :Invalid key",
            ":s 765 subspace status :Invalid value",
            ":s 767 subspace status :Limit",
            ":s 768 subspace status :Denied",
            ":s 769 subspace status :End",
            ":s 421 subspace METADATA :Unknown command",
            ":s 461 subspace METADATA :Need more parameters",
            ":s 774 * *ALL 0 :Try again later",
        ],
    );
    assert!(out.events.is_empty());
    assert_eq!(sent(&out), ["METADATA *ALL SYNC"]);
}

#[test]
fn delayed_sync_waits_and_stops_when_metadata_is_removed() {
    let mut session = registered(CAPS);
    let out = feed(&mut session, &[":s 774 * #c 30 :Try again later"]);
    assert!(out.send.is_empty());
    assert!(
        session
            .tick(now() + chrono::Duration::seconds(29))
            .send
            .is_empty()
    );
    assert_eq!(
        sent(&session.tick(now() + chrono::Duration::seconds(30))),
        ["METADATA #c SYNC"]
    );
    feed(
        &mut session,
        &[
            ":s 774 * *ALL 30 :Later",
            ":s CAP subspace DEL draft/metadata-2",
        ],
    );
    assert!(
        session
            .tick(now() + chrono::Duration::seconds(60))
            .send
            .is_empty()
    );
}

#[test]
fn malformed_unrelated_and_unnegotiated_values_are_ignored() {
    let mut session = registered(CAPS);
    let out = feed(
        &mut session,
        &[
            ":s 761 * alice status *",
            ":s 761 * alice other * :No",
            ":s 761 * #c status * :No",
            ":s METADATA alice status",
            ":s METADATA * status * :No",
            ":s METADATA alice OTHER * :No",
            ":s 766 * alice",
            ":s 774 * *ALL invalid :Later",
        ],
    );
    assert!(out.events.is_empty());
    assert!(out.send.is_empty());
    let mut legacy = registered("");
    assert!(
        feed(
            &mut legacy,
            &[
                ":s 761 * alice status * :No",
                ":s METADATA alice status * :No",
                ":s 774 * *ALL 0 :Later"
            ]
        )
        .events
        .is_empty()
    );
}
