//! Officer activity: decoding the route's and the event's shapes, joining
//! an item's pages, and joining the snapshot with the events.

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use subspace_framework::core::Secret;

use super::{
    ActivityChange, ActivityContent, ActivityEntry, ActivityItem, ActivityJoin, ActivityNews,
    AgentActivity, AgentActivityEvent, ItemPage, TurnStatus, join_pages,
};
use crate::events::{FleetEvent, Frames};
use crate::model::Role;
use crate::{Error, FleetAddress, FleetClient};

const AGENT: &str = "example-lyra";

fn turn(id: &str, seq: u64, status: &str, items: Value) -> Value {
    json!({
        "id": id, "seq": seq, "status": status, "startedAt": 1000, "updatedAt": 2000,
        "endedAt": null, "reconnects": 0, "usage": null, "items": items
    })
}

fn text(id: &str, seq: u64, words: &str) -> Value {
    json!({"id": id, "seq": seq, "startedAt": 1, "updatedAt": 2, "kind": "text",
           "status": "streaming", "text": words})
}

/// A snapshot shaped as stargate's `V1AgentActivity`, with example names.
fn snapshot_json(generation: &str, seq: u64) -> Value {
    json!({
        "agent": {"id": AGENT, "role": "captain", "backend": "opencode", "collected": true},
        "turns": [
            turn("t-old", 3, "completed", json!([])),
            turn("t-now", seq, "running", json!([
                {"id": "i1", "seq": 4, "startedAt": 1, "updatedAt": 2, "kind": "step",
                 "status": "completed", "model": {"provider": "example", "id": "model-a", "variant": null},
                 "firstTokenAt": 1, "streamedAt": 2, "endedAt": 3, "finish": "tool-calls",
                 "tokens": {"input": 10, "output": 5, "reasoning": 2, "cacheRead": 1, "cacheWrite": 0},
                 "cost": 0.25, "error": null},
                {"id": "i2", "seq": 5, "startedAt": 1, "updatedAt": 2, "kind": "tool",
                 "callId": "call-1", "name": "bash", "status": "completed",
                 "rawInput": "{\"command\":\"ls\"}", "input": {"command": "ls"}, "executed": true,
                 "metadata": {"output": "a\nb"},
                 "content": [{"type": "text", "text": "a\nb"},
                             {"type": "file", "uri": "data:text/plain,x", "mime": "text/plain", "name": null},
                             {"type": "hologram"}],
                 "error": null},
                {"id": "i3", "seq": 6, "startedAt": 1, "updatedAt": 2, "kind": "tool",
                 "status": "completed", "callId": "call-2", "name": "read",
                 "paged": {"pages": 3, "bytes": 140000}},
                text("i4", seq, "Hello"),
                {"id": "i5", "seq": 2, "startedAt": 1, "updatedAt": 2, "kind": "hologram"}
            ]))
        ],
        "cursor": {"generation": generation, "seq": seq}
    })
}

fn snapshot(generation: &str, seq: u64) -> AgentActivity {
    serde_json::from_value(snapshot_json(generation, seq)).unwrap()
}

fn event(generation: &str, seq: u64, change: Value) -> AgentActivityEvent {
    serde_json::from_value(json!({
        "agentId": AGENT, "turnId": "t-now", "generation": generation, "seq": seq,
        "at": 1_730_000_000_000_i64, "change": change
    }))
    .unwrap()
}

fn item_event(generation: &str, seq: u64, id: &str, words: &str) -> AgentActivityEvent {
    event(
        generation,
        seq,
        json!({"type": "item", "item": text(id, seq, words)}),
    )
}

fn words_of(join: &ActivityJoin, id: &str) -> Option<String> {
    let turns = &join.activity()?.turns;
    turns
        .iter()
        .flat_map(|turn| &turn.items)
        .find_map(|item| match item {
            ActivityEntry::Item(item) => match &**item {
                ActivityItem::Text(text) if text.base.id == id => Some(text.text.clone()),
                _ => None,
            },
            _ => None,
        })
}

fn news(connection: Option<u64>, events: Vec<AgentActivityEvent>, next: u64) -> ActivityNews {
    ActivityNews {
        connection,
        events,
        next,
        ..ActivityNews::default()
    }
}

#[test]
fn the_snapshot_decodes_every_item_kind_and_keeps_newlines() {
    let activity = snapshot("g1", 9);
    assert_eq!(activity.agent.role, Role::Captain);
    assert!(activity.agent.collected);
    assert_eq!(activity.cursor.seq, 9);
    let now = &activity.turns[1];
    assert_eq!(now.summary.status, TurnStatus::Running);
    let ActivityEntry::Item(step) = &now.items[0] else {
        panic!("an item");
    };
    let ActivityItem::Step(step) = &**step else {
        panic!("a step");
    };
    assert_eq!(step.tokens.map(|tokens| tokens.cache_read), Some(1));
    assert_eq!(step.cost, Some(0.25));
    let ActivityEntry::Item(tool) = &now.items[1] else {
        panic!("an item");
    };
    let ActivityItem::Tool(tool) = &**tool else {
        panic!("a tool call");
    };
    assert_eq!(tool.name.as_deref(), Some("bash"));
    assert_eq!(tool.input, Some(json!({"command": "ls"})));
    let content = tool.content.as_deref().unwrap();
    assert!(matches!(&content[0], ActivityContent::Text { text } if text == "a\nb"));
    assert!(matches!(&content[1], ActivityContent::File { mime, .. } if mime == "text/plain"));
    assert!(
        matches!(&content[2], ActivityContent::Other(raw) if *raw == json!({"type": "hologram"}))
    );
    let ActivityEntry::Paged(paged) = &now.items[2] else {
        panic!("a paged item");
    };
    assert_eq!((paged.paged.pages, paged.paged.bytes), (3, 140_000));
    assert_eq!(paged.call_id.as_deref(), Some("call-2"));
    assert!(matches!(&now.items[4], ActivityEntry::Unknown(item) if item.kind == "hologram"));
}

#[test]
fn an_uncollected_backend_decodes_with_no_turns() {
    let activity: AgentActivity = serde_json::from_value(json!({
        "agent": {"id": AGENT, "role": "rear_admiral", "backend": "claude", "collected": false},
        "turns": [], "cursor": {"generation": "g1", "seq": 0}
    }))
    .unwrap();
    assert!(!activity.agent.collected);
    assert_eq!(activity.agent.role, Role::RearAdmiral);
}

#[test]
fn the_agent_activity_event_decodes_from_the_stream() {
    let mut frames = Frames::default();
    let data = serde_json::to_string(&json!({
        "agentId": AGENT, "turnId": "t-now", "generation": "g1", "seq": 42, "at": 5,
        "change": {"type": "turn-ended", "turn": {
            "id": "t-now", "seq": 42, "status": "failed", "startedAt": 1, "updatedAt": 5,
            "endedAt": 5, "reconnects": 2,
            "usage": {"steps": 2, "tokens": {"input": 3, "output": 4, "reasoning": 0,
                                             "cacheRead": 0, "cacheWrite": 0}, "cost": 0.5}}}
    }))
    .unwrap();
    frames
        .push(format!("event: agent-activity\ndata: {data}\n\n").as_bytes())
        .unwrap();
    let Some(FleetEvent::AgentActivity(event)) = frames.ready.pop_front() else {
        panic!("an agent-activity event");
    };
    assert_eq!((event.agent_id.as_str(), event.seq), (AGENT, 42));
    let ActivityChange::TurnEnded { turn } = &event.change else {
        panic!("turn-ended");
    };
    assert_eq!(turn.status, TurnStatus::Failed);
    assert_eq!(turn.reconnects, 2);
    assert_eq!(turn.usage.as_ref().map(|usage| usage.steps), Some(2));
    // The debug form names the change, never its content.
    assert!(!format!("{event:?}").contains("usage"));
    let item = item_event("g1", 43, "i9", "secret words");
    assert!(matches!(item.change, ActivityChange::Item { .. }));
    assert!(!format!("{item:?}").contains("secret words"));
}

fn page(seq: u64, page: u32, pages: u32, data: &str, bytes: u64) -> ItemPage {
    ItemPage {
        agent: AGENT.into(),
        turn: "t-now".into(),
        item: "i3".into(),
        seq,
        page,
        pages,
        bytes,
        data: data.into(),
    }
}

#[test]
fn pages_join_into_the_whole_item_and_restart_when_it_changes() {
    let whole = r#"{"id":"i3"}"#;
    let (a, b) = whole.split_at(5);
    let reads = Mutex::new(Vec::new());
    // Version 1 changes to version 2 after page 0 was read.
    let joined = join_pages(&AtomicBool::new(false), |number| {
        let mut reads = reads.lock().unwrap();
        reads.push(number);
        let version = if reads.len() == 1 { 1 } else { 2 };
        let data = if number == 0 { a } else { b };
        Ok(page(version, number, 2, data, whole.len() as u64))
    })
    .unwrap()
    .unwrap();
    assert_eq!(joined, whole);
    assert_eq!(*reads.lock().unwrap(), [0, 1, 0, 1]);
}

#[test]
fn a_join_never_cuts_and_gives_up_on_an_item_that_keeps_changing() {
    let go = AtomicBool::new(false);
    let short = join_pages(&go, |number| Ok(page(1, number, 1, "{}", 9)));
    assert_eq!(short, Some(Err(Error::InvalidResponse)));
    let mut version = 0;
    let busy = join_pages(&go, |number| {
        version += 1;
        Ok(page(version, number, 2, "{", 2))
    });
    assert_eq!(busy, Some(Err(Error::Busy(None))));
}

#[test]
fn a_page_0_that_claims_more_than_an_item_can_hold_is_refused_before_any_other_read() {
    let go = AtomicBool::new(false);
    let reads = Mutex::new(0);
    let huge = join_pages(&go, |number| {
        *reads.lock().unwrap() += 1;
        Ok(page(1, number, 1_000_000, "{", 60_000_000_000))
    });
    assert_eq!(huge, Some(Err(Error::InvalidResponse)));
    assert_eq!(*reads.lock().unwrap(), 1, "no page after page 0");
    // No pages, more bytes than the pages hold, more pages than bytes, a
    // page past the declared size, or one with nothing in it.
    let refused = [
        vec![page(1, 0, 0, "", 0)],
        vec![page(1, 0, 2, "{}", 2 * 64 * 1024 + 1)],
        vec![page(1, 0, 5, "{", 2)],
        vec![page(1, 0, 2, "{", 2), page(1, 1, 2, "}}", 2)],
        vec![page(1, 0, 3, "{", 3), page(1, 1, 3, "", 3)],
    ];
    for pages in refused {
        let joined = join_pages(&go, |number| {
            Ok(pages
                .get(number as usize)
                .cloned()
                .unwrap_or_else(|| page(1, number, 1, "", 0)))
        });
        assert_eq!(joined, Some(Err(Error::InvalidResponse)));
    }
}

#[test]
fn a_cancelled_join_reads_no_page_after_the_flag_is_set() {
    let cancel = AtomicBool::new(false);
    let reads = Mutex::new(Vec::new());
    let joined = join_pages(&cancel, |number| {
        reads.lock().unwrap().push(number);
        // The view is closed while page 0 is being read.
        cancel.store(true, Ordering::Release);
        Ok(page(1, number, 3, "{", 3))
    });
    assert!(joined.is_none());
    assert_eq!(*reads.lock().unwrap(), [0]);
    assert!(join_pages(&cancel, |_| unreachable!("never read")).is_none());
}

#[test]
fn an_unknown_item_and_content_keep_their_json() {
    let turn: super::ActivityTurn = serde_json::from_value(turn(
        "t-now",
        9,
        "running",
        json!([
            {"id": "i7", "seq": 7, "startedAt": 1, "updatedAt": 2, "kind": "tool",
             "status": 7, "sentinel": "UNKNOWN-SHAPE"},
            {"id": "i8", "seq": 8, "startedAt": 1, "updatedAt": 2, "kind": "tool",
             "callId": "call-3", "name": "look", "status": "completed",
             "content": [{"type": "image", "data": "IMAGE-SENTINEL"}],
             "session": {"id": "s1", "parent": null}, "unavailable": false}
        ]),
    ))
    .unwrap();
    let ActivityEntry::Unknown(unknown) = &turn.items[0] else {
        panic!("a tool item of an unknown shape");
    };
    assert_eq!(unknown.kind, "tool");
    assert_eq!(unknown.raw["status"], json!(7));
    assert_eq!(unknown.raw["sentinel"], json!("UNKNOWN-SHAPE"));
    let ActivityEntry::Item(item) = &turn.items[1] else {
        panic!("a tool item with unknown optional fields");
    };
    let ActivityItem::Tool(tool) = &**item else {
        panic!("a tool call");
    };
    assert!(matches!(
        tool.content.as_deref(),
        Some([ActivityContent::Other(raw)]) if raw["data"] == json!("IMAGE-SENTINEL")
    ));
}

mod join;
mod routes;
