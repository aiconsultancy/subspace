//! The real hub answers tool calls on its drain and sends what they ask
//! through its own runtime, the one the board shows.

use std::sync::mpsc;

use gpui_kit::TestAppContext;
use serde_json::json;
use subspace_client_irc::Command;

use super::super::Request;
use super::{events, message_to};
use crate::config::Settings;
use crate::fixtures::{at, channel, config};
use crate::hub;

#[gpui_kit::test]
fn the_hub_answers_tool_calls_from_the_state_the_board_shows(cx: &mut TestAppContext) {
    let settings = Settings {
        networks: vec![config()],
        highlights: Vec::new(),
        ..Settings::default()
    };
    let hub = cx.update(|cx| hub::install_for_test(&settings, cx));
    let (requests, received) = mpsc::channel();
    hub.update(cx, |hub, cx| {
        hub.take_tools(received);
        hub.ingest(events(), at(30), cx);
    });
    let (read_reply, read) = mpsc::channel();
    let (write_reply, written) = mpsc::channel();
    let networks = serde_json::from_value(json!({"action":"networks"})).unwrap();
    requests.send(Request::Read(networks, read_reply)).unwrap();
    requests
        .send(Request::Write(
            message_to("#subspace", "hello"),
            write_reply,
        ))
        .unwrap();
    hub.update(cx, |hub, cx| hub.serve_tools(cx));
    let networks = read.try_recv().unwrap().unwrap();
    assert_eq!(networks["networks"][0]["nick"], "subspace");
    assert_eq!(written.try_recv().unwrap().unwrap()["status"], "queued");
    let sent = hub.read_with(cx, |hub, _| hub.sent().to_vec());
    assert_eq!(
        sent.last(),
        Some(&Command::Say {
            buffer: channel("#subspace"),
            text: "hello".into(),
            action: false,
        })
    );
}
