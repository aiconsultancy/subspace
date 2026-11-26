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
    AgentActivity, AgentActivityEvent, ItemPage, Joined, PartFailed, TurnStatus, decode_item,
    join_pages, read_part,
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
    assert!(matches!(&content[0], ActivityContent::Text { text, .. } if text == "a\nb"));
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

/// Round 5 thermo (P2), SUB decision `33ccecf6` item 4: a known shape
/// keeps every field this version does not know, read inline or joined
/// from pages alike, so the view can still show it.
#[test]
fn a_known_shape_keeps_the_fields_it_does_not_know() {
    let value = json!({
        "id": "i3", "seq": 6, "startedAt": 1, "updatedAt": 2, "kind": "tool", "callId": "c",
        "status": "completed", "sandbox": "on",
        "content": [{"type": "text", "text": "seen", "detail": "kept"},
                    {"type": "file", "uri": "data:,", "mime": "text/plain", "size": 3}],
        "error": {"type": "Timeout", "message": "slow", "hint": "retry"}
    });
    let check = |entry: ActivityEntry| {
        let ActivityEntry::Item(item) = entry else {
            panic!("an item");
        };
        let ActivityItem::Tool(tool) = *item else {
            panic!("a tool call");
        };
        assert_eq!(tool.extra.get("sandbox"), Some(&json!("on")));
        assert!(!tool.extra.contains_key("id") && !tool.extra.contains_key("kind"));
        let content = tool.content.as_deref().unwrap();
        assert!(matches!(&content[0], ActivityContent::Text { text, extra }
            if text == "seen" && extra.get("detail") == Some(&json!("kept")) && extra.len() == 1));
        assert!(matches!(&content[1], ActivityContent::File { extra, .. }
            if extra.get("size") == Some(&json!(3)) && extra.len() == 1));
        assert_eq!(tool.error.unwrap().extra.get("hint"), Some(&json!("retry")));
    };
    check(serde_json::from_value(value.clone()).unwrap());
    check(decode_item(&value.to_string(), "i3", 6).ok().unwrap());
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
mod pages;
mod routes;
