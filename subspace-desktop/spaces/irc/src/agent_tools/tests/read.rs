//! `irc_read` output from fixture state.

use serde_json::{Value, json};
use subspace_client_irc::Command;

use super::super::read::{self, Request as Read};
use super::state;
use crate::fixtures::{at, channel};
use crate::state::IrcState;

fn read(state: &mut IrcState, request: Value) -> (Result<Value, String>, Vec<Command>) {
    let request: Read = serde_json::from_value(request).unwrap();
    read::answer(state, request)
}

fn ok(state: &mut IrcState, request: Value) -> Value {
    read(state, request).0.unwrap()
}

#[test]
fn networks_and_buffers_are_compact_and_plain() {
    let mut state = state();
    let networks = ok(&mut state, json!({"action":"networks"}));
    assert_eq!(
        networks["networks"][0],
        json!({"id":"local","name":"Acme IRC","status":"Connected","connected":true,"nick":"subspace","chathistory":true})
    );
    let buffers = ok(&mut state, json!({"action":"buffers","network":"local"}));
    let room = buffers["buffers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|buffer| buffer["target"] == "#subspace")
        .unwrap();
    assert_eq!(room["kind"], "channel");
    assert_eq!(room["topic"], "Fixture channel");
    assert_eq!(room["members"], 2);
    assert_eq!(room["unread"], 3);
    assert_eq!(room["lines"], 4);
    let error = read(
        &mut state,
        json!({"action":"buffers","network":"elsewhere"}),
    )
    .0
    .unwrap_err();
    assert!(error.contains("Configured networks: local"), "{error}");
}

#[test]
fn messages_page_back_then_ask_the_server_for_older_history() {
    let mut state = state();
    let (page, commands) = read(
        &mut state,
        json!({"action":"messages","network":"local","target":"#SUBSPACE","limit":2}),
    );
    let page = page.unwrap();
    assert!(commands.is_empty());
    assert_eq!(page["target"], "#subspace");
    assert_eq!(page["lines"].as_array().unwrap().len(), 2);
    assert_eq!(page["lines"][0]["text"], "rocket ready?");
    assert_eq!(page["lines"][0]["kind"], "message");
    assert_eq!(page["lines"][0]["time"], "2026-11-13T09:03:00Z");
    assert_eq!(page["server_history"], "available");
    let before = page["next_before"].clone();

    let (older, commands) = read(
        &mut state,
        json!({"action":"messages","network":"local","target":"#subspace","before":before}),
    );
    let older = older.unwrap();
    assert_eq!(older["lines"][1]["kind"], "join");
    assert_eq!(older["server_history"], "requested");
    assert_eq!(
        commands,
        [Command::LoadHistory {
            buffer: channel("#subspace"),
            before: Some(at(1)),
            limit: 100,
        }]
    );
    let (again, commands) = read(
        &mut state,
        json!({"action":"messages","network":"local","target":"#subspace","before":before}),
    );
    assert_eq!(again.unwrap()["server_history"], "loading");
    assert!(commands.is_empty());

    let server = ok(&mut state, json!({"action":"messages","network":"local"}));
    assert_eq!(server["target"], Value::Null);
    assert_eq!(server["server_history"], "unavailable");
    let missing = read(
        &mut state,
        json!({"action":"messages","network":"local","target":"#elsewhere"}),
    );
    assert!(missing.0.unwrap_err().contains("no buffer #elsewhere"));
}

#[test]
fn members_and_search_read_only_what_the_board_holds() {
    let mut state = state();
    let members = ok(
        &mut state,
        json!({"action":"members","network":"local","channel":"#subspace"}),
    );
    assert_eq!(members["members"][0]["nick"], "alice");
    assert_eq!(members["members"][0]["modes"], "@");
    assert_eq!(members["members"][1]["modes"], "+");
    let found = ok(
        &mut state,
        json!({"action":"search","query":"ROCKET","limit":1}),
    );
    assert_eq!(found["matches"][0]["target"], "carol");
    assert_eq!(found["matches"][0]["text"], "rocket launch at noon");
    assert_eq!(found["truncated"], true);
    assert!(
        read(&mut state, json!({"action":"search","query":"  "}))
            .0
            .is_err()
    );
    let unknown: Result<Read, _> = serde_json::from_value(json!({"action":"raw","line":"QUIT"}));
    assert!(unknown.is_err());
}
