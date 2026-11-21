//! The record mapping and the store round trip against a temporary store.

use chrono::Duration;
use subspace_client_irc::{Line, LineKind};
use subspace_framework::dashboard::full_text::{Filters, Policy, RecordQuery, Service};

use super::mapping::{self, CHANNEL_KIND, PRIVATE_KIND};
use super::{Link, Op};
use crate::fixtures::{at, channel, line, message, query};
use crate::thread_status::Kept;

/// A store in a temporary folder with IRC's source on, as IRC declares.
pub(crate) fn service() -> (tempfile::TempDir, Service) {
    let root = tempfile::tempdir().unwrap();
    let service = Service::open(
        root.path().join("search"),
        "fixture".into(),
        vec!["irc".into()],
    )
    .unwrap();
    service
        .offer_default(
            "irc",
            Policy {
                enabled: true,
                ..Policy::default()
            },
        )
        .unwrap();
    (root, service)
}

/// `line` moved to two hours ago plus its fixture minutes, within the
/// store's retention whenever the tests run.
pub(crate) fn recent(mut line: Line) -> Line {
    line.time = chrono::Utc::now() - Duration::hours(2) + (line.time - at(0));
    line
}

fn recent_message(
    buffer: &subspace_client_irc::BufferKey,
    from: &str,
    text: &str,
    minute: i64,
) -> Line {
    recent(message(buffer, from, text, minute))
}

fn with_id(mut line: Line, id: &str) -> Line {
    line.id = Some(id.into());
    line
}

#[test]
fn reply_relationships_survive_the_store_mapping_and_late_parent_restore() {
    for key in [channel("#subspace"), query("bob")] {
        let mut reply = with_id(message(&key, "bob", "child", 2), "child");
        reply.reply_to = Some("middle".into());
        let mut middle = with_id(message(&key, "bob", "middle", 1), "middle");
        middle.reply_to = Some("root".into());
        let record = mapping::document(&reply, at(60)).unwrap();
        let restored = mapping::line(&record).unwrap();
        assert_eq!(restored, reply);
        assert_eq!(restored.thread_root(&[&restored]), Some("middle"));
        let mut state = crate::state::IrcState::new(&[crate::fixtures::config()], Vec::new());
        state.restore(&key, vec![restored]);
        state.restore(&key, vec![middle]);
        let held: Vec<&Line> = state
            .buffer(&key)
            .unwrap()
            .entries
            .iter()
            .map(|entry| &entry.line)
            .collect();
        let child = held
            .iter()
            .find(|line| line.id.as_deref() == Some("child"))
            .unwrap();
        assert_eq!(child.thread_root(&held), Some("root"));
        assert_eq!(state.buffer(&key).unwrap().entries.len(), 2);
    }
}

fn count(link: &Link, parent: Option<String>) -> usize {
    let query = RecordQuery {
        filters: Filters {
            parent_id: parent,
            ..Default::default()
        },
        before: None,
        limit: 1000,
    };
    link.service
        .records(&link.scope, &query)
        .map_or(0, |page| page.documents.len())
}

#[test]
fn a_message_maps_to_one_record_with_its_filters_and_plain_text() {
    let room = channel("#Subspace");
    let line = with_id(
        message(
            &room,
            "alice",
            "\u{2}bold\u{2} https://github.com/octo-org/api/pull/42",
            1,
        ),
        "m1",
    );
    let record = mapping::document(&line, at(60)).unwrap();
    assert_eq!(record.id, "msgid:local:m1");
    assert_eq!(record.kind, CHANNEL_KIND);
    assert_eq!(record.title, "#Subspace · alice");
    assert_eq!(record.body, "bold https://github.com/octo-org/api/pull/42");
    assert_eq!(
        record.parent_id.as_deref(),
        Some("buffer:local:c:#subspace")
    );
    assert_eq!(record.refreshed_at, at(1).timestamp());
    assert_eq!(mapping::line(&record), Some(line.clone()));
    let navigation = mapping::navigation(&record).unwrap();
    assert_eq!(navigation.buffer(), room);
    assert_eq!(navigation.msgid.as_deref(), Some("m1"));
    let metadata = mapping::metadata(&line);
    assert_eq!(metadata.project.as_deref(), Some("local"));
    assert_eq!(metadata.references.len(), 1);
    let private = mapping::document(&message(&query("carol"), "carol", "hi", 2), at(60)).unwrap();
    assert_eq!(
        (private.kind.as_str(), private.title.as_str()),
        (PRIVATE_KIND, "carol")
    );
}

#[test]
fn ids_without_a_msgid_are_stable_hashes_and_only_messages_are_stored() {
    let room = channel("#subspace");
    let hello = message(&room, "alice", "hello", 1);
    let id = mapping::document_id(&hello);
    assert!(id.starts_with("line:"), "{id}");
    assert_eq!(
        id,
        mapping::document_id(&message(&channel("#SUBSPACE"), "Alice", "hello", 1))
    );
    assert_ne!(
        id,
        mapping::document_id(&message(&room, "alice", "hello", 2))
    );
    assert!(mapping::document(&line(&room, LineKind::Join, "bob", "joined", 1), at(9)).is_none());
    let server = crate::state::server_key(&crate::fixtures::network_id());
    assert!(mapping::document(&message(&server, "", "motd", 1), at(9)).is_none());
    let ahead = mapping::document(&message(&room, "alice", "skewed", 10), at(5)).unwrap();
    assert_eq!(ahead.refreshed_at, at(5).timestamp(), "never ahead of now");
}

#[test]
fn stored_lines_restore_in_order_per_buffer_and_forget_deletes_one_buffer() {
    let (_root, service) = service();
    let link = Link::new(service);
    let room = channel("#subspace");
    let carol = query("carol");
    let mut lines: Vec<Line> = (0..150)
        .map(|minute| recent_message(&room, "alice", &format!("line {minute}"), minute))
        .collect();
    lines.push(recent_message(&carol, "carol", "private", 3));
    lines.push(recent(line(&room, LineKind::Part, "bob", "left", 4)));
    let now = chrono::Utc::now();
    assert!(link.run(vec![Op::Publish(lines)], now).is_empty());
    assert_eq!(count(&link, None), 151, "membership lines are not stored");
    let restored = link.restore().unwrap().unwrap();
    let held = restored
        .iter()
        .find(|restored| restored.buffer == room)
        .unwrap();
    assert!(
        held.requests.is_empty(),
        "a store without requests loads none"
    );
    let room_lines = &held.lines;
    assert_eq!(room_lines.len(), 150);
    assert_eq!(room_lines[0].text, "line 0");
    assert_eq!(room_lines[149].text, "line 149");
    assert!(link.run(vec![Op::Forget(room.clone())], now).is_empty());
    assert_eq!(count(&link, None), 1);
    assert_eq!(count(&link, Some(mapping::parent_id(&carol))), 1);
}

#[test]
fn request_bindings_and_endings_round_trip_with_their_buffer() {
    let (_root, service) = service();
    let link = Link::new(service);
    let room = channel("#subspace");
    let now = chrono::Utc::now();
    let kept = vec![
        Kept {
            request: "first".into(),
            pin: Some("fleet-astra".into()),
            ended: false,
        },
        Kept {
            request: "second".into(),
            pin: Some("fleet-astra".into()),
            ended: true,
        },
    ];
    let ops = vec![
        Op::Publish(vec![recent_message(&room, "alice", "hi", 1)]),
        Op::Requests(room.clone(), kept.clone()),
    ];
    assert!(link.run(ops, now).is_empty());
    let restored = link.restore().unwrap().unwrap();
    let held = restored
        .iter()
        .find(|restored| restored.buffer == room)
        .unwrap();
    assert_eq!(held.lines.len(), 1, "a request record is not a line");
    for one in &kept {
        assert!(held.requests.contains(one), "{one:?}");
    }
    assert!(link.run(vec![Op::Forget(room.clone())], now).is_empty());
    assert_eq!(count(&link, None), 0, "the buffer's requests go with it");
}

#[test]
fn with_the_source_off_nothing_is_stored_or_restored() {
    let (_root, service) = service();
    service.set_policy("irc", Policy::default()).unwrap();
    let link = Link::new(service);
    let room = channel("#subspace");
    let now = chrono::Utc::now();
    assert!(
        link.run(
            vec![Op::Publish(vec![recent_message(&room, "alice", "hi", 1)])],
            now
        )
        .is_empty()
    );
    assert_eq!(link.restore().unwrap(), None);
}

#[test]
fn a_cleared_store_takes_new_messages_again() {
    let (_root, service) = service();
    let link = Link::new(service.clone());
    let room = channel("#subspace");
    let now = chrono::Utc::now();
    link.run(
        vec![Op::Publish(vec![recent_message(&room, "alice", "old", 1)])],
        now,
    );
    service.clear("irc").unwrap();
    assert!(
        link.run(
            vec![Op::Publish(vec![recent_message(&room, "alice", "new", 2)])],
            now
        )
        .is_empty()
    );
    let restored = link.restore().unwrap().unwrap();
    let texts: Vec<&str> = restored[0]
        .lines
        .iter()
        .map(|line| line.text.as_str())
        .collect();
    assert_eq!(texts, ["new"]);
}

#[cfg(unix)]
#[test]
fn the_store_file_is_owner_only() {
    use std::os::unix::fs::PermissionsExt as _;
    let (root, service) = service();
    let link = Link::new(service);
    link.run(
        vec![Op::Publish(vec![recent_message(
            &channel("#subspace"),
            "alice",
            "secret",
            1,
        )])],
        chrono::Utc::now(),
    );
    let files: Vec<_> = walk(root.path())
        .into_iter()
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "sqlite")
        })
        .collect();
    assert_eq!(files.len(), 1);
    let mode = std::fs::metadata(&files[0]).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[cfg(unix)]
fn walk(directory: &std::path::Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .flat_map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                walk(&path)
            } else {
                vec![path]
            }
        })
        .collect()
}
