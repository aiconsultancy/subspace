use std::collections::{HashSet, VecDeque};

use chrono::Utc;
use subspace_client_irc::LineKind;

use chrono::NaiveDate;

use super::{Item, day_label, item_id, items, membership_summary};
use crate::fixtures::{channel, line, message};
use crate::state::Entry;

fn entries(lines: Vec<subspace_client_irc::Line>) -> VecDeque<Entry> {
    lines
        .into_iter()
        .zip(0..)
        .map(|(line, seq)| Entry {
            seq,
            line,
            highlight: false,
        })
        .collect()
}

#[test]
fn joins_parts_quits_and_nick_changes_condense_into_one_row() {
    let room = channel("#subspace");
    let held = entries(vec![
        line(&room, LineKind::Join, "alice", "", 0),
        line(&room, LineKind::Join, "bob", "", 1),
        line(&room, LineKind::Part, "carol", "left (bye)", 2),
        line(&room, LineKind::Nick, "dave", "is now known as dave2", 3),
        line(&room, LineKind::Quit, "erin", "quit (Ping timeout)", 4),
        line(&room, LineKind::Join, "alice", "", 5),
        message(&room, "bob", "hello", 6),
        line(&room, LineKind::Join, "frank", "", 7),
    ]);
    let rows = items(&held, &HashSet::new(), &Utc, true);
    assert_eq!(rows.len(), 4);
    assert!(matches!(rows[0], Item::Day(_)));
    assert_eq!(rows[1], Item::Membership { first: 0, last: 5 });
    assert_eq!(
        rows[2],
        Item::Message {
            at: 6,
            continues: false
        }
    );
    assert_eq!(rows[3], Item::Membership { first: 7, last: 7 });
    let run: Vec<&Entry> = held.range(0..=5).collect();
    assert_eq!(
        membership_summary(&run),
        "alice, bob joined · carol left · dave → dave2 · erin quit"
    );
}

#[test]
fn a_new_day_starts_with_a_separator() {
    let room = channel("#subspace");
    let minutes_per_day = 24 * 60;
    let held = entries(vec![
        message(&room, "alice", "today", 0),
        message(&room, "alice", "tomorrow", minutes_per_day),
    ]);
    let rows = items(&held, &HashSet::new(), &Utc, true);
    assert!(matches!(
        rows.as_slice(),
        [
            Item::Day(_),
            Item::Message {
                at: 0,
                continues: false
            },
            Item::Day(_),
            Item::Message {
                at: 1,
                continues: false
            }
        ]
    ));
}

#[test]
fn a_growing_run_changes_identity_and_lines_keep_theirs() {
    let room = channel("#subspace");
    let mut held = entries(vec![
        message(&room, "alice", "hi", 0),
        line(&room, LineKind::Join, "bob", "", 1),
    ]);
    let before: Vec<String> = items(&held, &HashSet::new(), &Utc, true)
        .into_iter()
        .map(|item| item_id(item, &held))
        .collect();
    held.push_back(Entry {
        seq: 2,
        line: line(&room, LineKind::Join, "carol", "", 2),
        highlight: false,
    });
    let after: Vec<String> = items(&held, &HashSet::new(), &Utc, true)
        .into_iter()
        .map(|item| item_id(item, &held))
        .collect();
    assert_eq!(before[..2], after[..2]);
    assert_eq!(before[2], "members:1:1");
    assert_eq!(after[2], "members:1:2");
}

fn message_rows(rows: &[Item]) -> Vec<(usize, bool)> {
    rows.iter()
        .filter_map(|row| match row {
            Item::Message { at, continues } => Some((*at, *continues)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_sender_s_messages_within_five_minutes_share_one_header() {
    let room = channel("#subspace");
    let held = entries(vec![
        message(&room, "alice", "one", 0),
        message(&room, "Alice", "two", 2),
        message(&room, "alice", "three", 7),
        message(&room, "alice", "much later", 13),
        message(&room, "bob", "hi", 14),
        line(&room, LineKind::Notice, "bob", "notice", 14),
    ]);
    let rows = items(&held, &HashSet::new(), &Utc, true);
    assert_eq!(
        message_rows(&rows),
        [
            (0, false),
            (1, true),
            (2, true),
            (3, false),
            (4, false),
            (5, false)
        ],
        "case-insensitive sender, the window counts from the previous message, a notice starts its own group"
    );
}

#[test]
fn system_events_and_a_new_day_break_a_group() {
    let room = channel("#subspace");
    // The fixture clock starts at 09:00 UTC: midnight is 15 hours later.
    let minutes_per_day = 15 * 60;
    let held = entries(vec![
        message(&room, "alice", "one", minutes_per_day - 2),
        message(&room, "alice", "after midnight", minutes_per_day + 1),
        line(&room, LineKind::Join, "bob", "", minutes_per_day + 1),
        message(&room, "alice", "after a join", minutes_per_day + 2),
        line(
            &room,
            LineKind::Topic,
            "bob",
            "changed the topic",
            minutes_per_day + 2,
        ),
        message(&room, "alice", "after a topic", minutes_per_day + 3),
    ]);
    let rows = items(&held, &HashSet::new(), &Utc, true);
    assert_eq!(
        message_rows(&rows),
        [(0, false), (1, false), (3, false), (5, false)]
    );
    assert!(
        rows.contains(&Item::Line(4)),
        "a topic change is a compact line"
    );
}

#[test]
fn a_server_buffer_keeps_every_line_compact() {
    let room = channel("#subspace");
    let held = entries(vec![
        message(&room, "alice", "one", 0),
        line(&room, LineKind::Info, "", "Welcome", 0),
    ]);
    let rows = items(&held, &HashSet::new(), &Utc, false);
    assert_eq!(rows[1..], [Item::Line(0), Item::Line(1)]);
}

#[test]
fn a_continued_message_has_its_own_identity() {
    let room = channel("#subspace");
    let held = entries(vec![
        message(&room, "alice", "one", 0),
        message(&room, "alice", "two", 1),
    ]);
    let ids: Vec<String> = items(&held, &HashSet::new(), &Utc, true)
        .into_iter()
        .map(|item| item_id(item, &held))
        .collect();
    assert_eq!(ids[1..], ["message:0", "message:1:more"]);
}

#[test]
fn day_labels_read_like_a_chat_client() {
    let date = |month, day| NaiveDate::from_ymd_opt(2026, month, day).unwrap_or_default();
    let today = date(11, 14);
    assert_eq!(day_label(today, today), "Today");
    assert_eq!(day_label(date(11, 13), today), "Yesterday");
    assert_eq!(day_label(date(11, 6), today), "Friday 6 November");
    let last_year = NaiveDate::from_ymd_opt(2025, 12, 31).unwrap_or_default();
    assert_eq!(day_label(last_year, today), "Wednesday 31 December 2025");
}

#[test]
fn folded_progress_makes_no_row_and_splits_no_sender_group() {
    let room = channel("#ops");
    let held = entries(vec![
        message(&room, "astra", "first", 0),
        line(&room, LineKind::Notice, "astra", "progress", 1),
        message(&room, "astra", "second", 2),
        line(&room, LineKind::Join, "bob", "", 3),
        line(&room, LineKind::Notice, "astra", "progress", 4),
        line(&room, LineKind::Join, "carol", "", 5),
    ]);
    let folded = HashSet::from([1, 4]);
    let rows = items(&held, &folded, &Utc, true);
    assert!(matches!(rows[0], Item::Day(_)));
    assert_eq!(
        &rows[1..],
        [
            Item::Message {
                at: 0,
                continues: false
            },
            Item::Message {
                at: 2,
                continues: true
            },
            Item::Membership { first: 3, last: 3 },
            Item::Membership { first: 5, last: 5 },
        ],
        "a membership run never spans a folded line"
    );
}
