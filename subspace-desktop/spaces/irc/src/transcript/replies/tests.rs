use subspace_client_irc::{BufferKey, Event, LineKind};

use super::{Summary, counts};
use crate::fixtures::{at, channel, config, message, query};
use crate::state::{Buffer, Entry, IrcState};

#[test]
fn count_changes_invalidate_the_root_height_without_replacing_reply_rows() {
    let room = channel("#threads");
    let mut held = Buffer::default();
    held.entries = [
        entry(&room, 0, "root", None),
        entry(&room, 1, "reply", None),
    ]
    .into();
    let rows = crate::transcript::rows(&held, &room.target, &chrono::Utc);
    let before = counts(&room, &held);
    held.entries[1].line.reply_to = Some("root".into());
    let after = counts(&room, &held);
    let root = crate::transcript::row_of(&rows, 0).unwrap();
    assert_eq!(
        super::changed_rows(&rows, &held.entries, &before, &after),
        [root],
        "remeasure only the changed root, even offscreen"
    );
    held.entries[1].line.time = at(4);
    let later = counts(&room, &held);
    assert_eq!(
        super::changed_rows(&rows, &held.entries, &after, &later),
        [root],
        "a longer metadata label may wrap"
    );
}

fn entry(buffer: &BufferKey, seq: i64, id: &str, parent: Option<&str>) -> Entry {
    let mut line = message(buffer, "astra", id, seq);
    line.id = Some(id.into());
    line.reply_to = parent.map(str::to_owned);
    Entry {
        seq,
        line,
        highlight: false,
    }
}

#[test]
fn cycles_empty_links_and_foreign_buffers_do_not_create_counts() {
    let room = channel("#threads");
    let other = query("astra");
    let mut held = Buffer::default();
    held.entries = [
        entry(&room, 0, "root", None),
        entry(&room, 1, "self", Some("self")),
        entry(&room, 2, "a", Some("b")),
        entry(&room, 3, "b", Some("a")),
        entry(&room, 4, "empty", Some("")),
        entry(&other, 5, "foreign", Some("root")),
    ]
    .into();
    assert!(counts(&room, &held).is_empty());
    assert!(counts(&crate::state::server_key(&room.network), &held).is_empty());
}

#[test]
fn trimming_replies_reduces_counts_and_trimming_the_root_removes_its_count() {
    let room = channel("#threads");
    let mut held = Buffer::default();
    held.entries = [
        entry(&room, 0, "root", None),
        entry(&room, 1, "reply", Some("root")),
        entry(&room, 2, "child", Some("reply")),
    ]
    .into();
    assert_eq!(
        counts(&room, &held).get(&0),
        Some(&Summary {
            count: 2,
            latest: at(2)
        })
    );
    held.entries.pop_back();
    assert_eq!(
        counts(&room, &held).get(&0),
        Some(&Summary {
            count: 1,
            latest: at(1)
        })
    );
    held.entries.pop_front();
    assert!(counts(&room, &held).is_empty());
}

#[test]
fn restored_history_rebuilds_the_same_counts_and_latest_time() {
    for room in [channel("#threads"), query("astra")] {
        let mut state = IrcState::new(&[config()], Vec::new());
        let mut entries = vec![
            entry(&room, 0, "root", None),
            entry(&room, 3, "late", Some("root")),
            entry(&room, 1, "early", Some("late")),
        ];
        entries[1].line.kind = LineKind::Notice;
        entries[2].line.kind = LineKind::Action;
        for entry in &entries {
            state.apply(Event::Line(entry.line.clone()), at(4));
        }
        let original = counts(&room, state.buffer(&room).unwrap());
        assert_eq!(
            original.values().next(),
            Some(&Summary {
                count: 2,
                latest: at(3)
            })
        );
        let payload =
            serde_json::to_string(&entries.iter().map(|entry| &entry.line).collect::<Vec<_>>())
                .unwrap();
        let mut restored = IrcState::new(&[config()], Vec::new());
        restored.restore(&room, serde_json::from_str(&payload).unwrap());
        let rebuilt = counts(&room, restored.buffer(&room).unwrap());
        assert_eq!(
            original.values().collect::<Vec<_>>(),
            rebuilt.values().collect::<Vec<_>>()
        );
    }
}
