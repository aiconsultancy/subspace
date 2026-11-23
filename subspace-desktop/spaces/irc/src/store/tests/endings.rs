//! Request endings in the store: committed with lines, restored with or
//! without them, deleted with their buffer, and older records ignored.

use subspace_client_irc::BufferKey;
use subspace_framework::dashboard::full_text::Document;

use super::super::mapping;
use super::super::{Ended, Link, Op};
use super::{count, recent_message, service};
use crate::fixtures::{channel, query};

/// An ending of request `request` in `buffer`, stored without its outcome
/// or time, as before they were kept.
fn ended(buffer: &BufferKey, request: &str) -> Ended {
    Ended {
        buffer: buffer.clone(),
        request: request.into(),
        outcome: None,
        at: None,
    }
}

#[test]
fn endings_commit_with_lines_and_restore_with_or_without_them() {
    let (_root, service) = service();
    let link = Link::new(service);
    let room = channel("#subspace");
    let carol = query("carol");
    let now = chrono::Utc::now();
    let ops = vec![
        Op::Commit {
            ended: vec![ended(&room, "first"), ended(&carol, "direct")],
            lines: vec![recent_message(&room, "alice", "hi", 1)],
        },
        Op::Commit {
            ended: vec![ended(&room, "second")],
            lines: Vec::new(),
        },
    ];
    assert!(link.run(ops, now).is_empty());
    let restored = link.restore().unwrap().unwrap();
    let held = restored
        .iter()
        .find(|restored| restored.buffer == room)
        .unwrap();
    assert_eq!(held.lines.len(), 1, "an ending record is not a line");
    let mut ids: Vec<&str> = held.ended.iter().map(|(id, _)| id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, ["first", "second"]);
    let private = restored
        .iter()
        .find(|restored| restored.buffer == carol)
        .unwrap();
    assert!(private.lines.is_empty(), "no line survives there");
    assert_eq!(
        private.ended,
        [("direct".to_owned(), crate::thread_status::Ending::default())]
    );
    assert!(link.run(vec![Op::Forget(room.clone())], now).is_empty());
    assert_eq!(count(&link, Some(mapping::parent_id(&room))), 0);
    assert_eq!(count(&link, None), 1, "the endings go with their buffer");
}

#[test]
fn an_older_request_record_loads_as_nothing() {
    let (_root, service) = service();
    let link = Link::new(service.clone());
    let room = channel("#subspace");
    let now = chrono::Utc::now();
    let older = Document {
        id: "request:local:first".into(),
        title: String::new(),
        body: String::new(),
        metadata: String::new(),
        parent_id: Some(mapping::parent_id(&room)),
        kind: "Channels request".into(),
        url: None,
        navigation: String::new(),
        refreshed_at: now.timestamp(),
        payload: Some(r#"{"request":"first","pin":"astra","ended":true}"#.into()),
    };
    let line = recent_message(&room, "alice", "hi", 1);
    assert!(link.run(vec![Op::Publish(vec![line])], now).is_empty());
    service.upsert(&link.scope, &[older]).unwrap();
    let restored = link.restore().unwrap().unwrap();
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].lines.len(), 1);
    assert!(restored[0].ended.is_empty());
}

#[test]
fn an_ending_keeps_its_outcome_and_server_time() {
    use crate::thread_status::{Ending, Outcome};
    let (_root, service) = service();
    let link = Link::new(service);
    let room = channel("#subspace");
    let now = chrono::Utc::now();
    let time = crate::fixtures::at(43);
    let mut ending = ended(&room, "first");
    ending.outcome = Some(Outcome::Completed);
    ending.at = Some(time);
    let ops = vec![Op::Commit {
        ended: vec![ending, ended(&room, "older")],
        lines: Vec::new(),
    }];
    assert!(link.run(ops, now).is_empty());
    let restored = link.restore().unwrap().unwrap();
    let mut held = restored[0].ended.clone();
    held.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        held,
        [
            (
                "first".to_owned(),
                Ending {
                    outcome: Some(Outcome::Completed),
                    at: Some(time),
                }
            ),
            ("older".to_owned(), Ending::default()),
        ],
        "an ending stored without them loads without them"
    );
}
