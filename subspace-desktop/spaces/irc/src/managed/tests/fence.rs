//! An account change fences the account's networks before the hub takes
//! anything more for them: a tool call queued before the change is not sent
//! as the previous account, and a message that arrives after it brings no
//! conversation back and raises nothing. Fictional accounts only.

use std::sync::mpsc;

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;
use subspace_client_irc::{BufferKey, Command, ConnectionStatus, Event, Target};

use super::support::{Api, TestEnv, configured, connect, fleets, member_of, settle};
use crate::agent_tools::Request;
use crate::fixtures::{at, message};
use crate::hub::Hub;
use crate::tests::{mount, settings};

/// Ada signed in, her account network connected and in `#aurora-nav`.
fn ada_connected(
    cx: &mut TestAppContext,
) -> (
    Entity<Hub>,
    Api,
    std::sync::Arc<TestEnv>,
    BufferKey,
    &mut VisualTestContext,
) {
    let (_board, hub, cx) = mount(cx, settings());
    let api = Api::new(&["127.0.0.1"]);
    let env = connect(&hub, &api, cx);
    member_of(&hub, (Some(1), "user-ada"), fleets("127.0.0.1", None), cx);
    settle(&hub, cx);
    let network = configured(&hub, cx)[1].id.clone();
    let nav = BufferKey {
        network: network.clone(),
        target: Target::Channel("#aurora-nav".into()),
    };
    let events = vec![
        Event::Status {
            network,
            status: ConnectionStatus::Connected { nick: "ada".into() },
        },
        Event::Joined {
            buffer: nav.clone(),
        },
    ];
    hub.update(cx, |hub, cx| hub.ingest(events, at(1), cx));
    (hub, api, env, nav, cx)
}

/// A tool's message to `buffer`'s channel, answered by the hub's next
/// serving of tool calls.
fn say(
    hub: &Entity<Hub>,
    buffer: &BufferKey,
    cx: &mut VisualTestContext,
) -> Result<serde_json::Value, String> {
    let (requests, received) = mpsc::channel();
    let (reply, answer) = mpsc::channel();
    let Target::Channel(channel) = &buffer.target else {
        unreachable!("a channel");
    };
    let write = serde_json::from_value(json!({"action": "message",
        "network": buffer.network.0, "target": channel, "text": "launch"}))
    .unwrap();
    requests.send(Request::Write(write, reply)).unwrap();
    hub.update(cx, |hub, cx| {
        hub.take_tools(received);
        hub.serve_tools(cx);
    });
    answer.try_recv().unwrap()
}

fn said(hub: &Entity<Hub>, buffer: &BufferKey, cx: &VisualTestContext) -> usize {
    hub.read_with(cx, |hub, _| {
        hub.sent()
            .iter()
            .filter(|command| matches!(command, Command::Say { buffer: to, .. } if to == buffer))
            .count()
    })
}

#[gpui_kit::test]
fn a_tool_call_queued_before_sign_out_is_not_sent_as_the_previous_account(cx: &mut TestAppContext) {
    let (hub, api, env, nav, cx) = ada_connected(cx);
    assert_eq!(say(&hub, &nav, cx).unwrap()["status"], "queued");
    assert_eq!(said(&hub, &nav, cx), 1);
    // Signed out; the hub's timer has not run since.
    api.sign_in(401, json!({}));
    *env.generation.lock().unwrap() = Some(2);
    let refused = say(&hub, &nav, cx).unwrap_err();
    assert!(refused.contains("No IRC network has the id"), "{refused}");
    assert_eq!(said(&hub, &nav, cx), 1, "nothing more went out");
    assert_eq!(configured(&hub, cx).len(), 1, "her network stopped first");
    // A command for her network from anywhere else goes nowhere too.
    hub.update(cx, |hub, _| {
        hub.send(Command::Say {
            buffer: nav.clone(),
            text: "launch".into(),
            action: false,
        })
    });
    assert_eq!(said(&hub, &nav, cx), 1);
}

#[gpui_kit::test]
fn a_message_after_sign_out_brings_back_no_conversation(cx: &mut TestAppContext) {
    let (hub, api, env, nav, cx) = ada_connected(cx);
    let lyra = BufferKey {
        network: nav.network.clone(),
        target: Target::Query("lyra".into()),
    };
    api.sign_in(401, json!({}));
    *env.generation.lock().unwrap() = Some(2);
    let late = vec![Event::Line(message(&lyra, "lyra", "are you there?", 2))];
    hub.update(cx, |hub, cx| hub.ingest(late.clone(), at(2), cx));
    hub.read_with(cx, |hub, _| {
        assert!(hub.state.buffer(&lyra).is_none(), "no conversation");
        assert!(hub.state.buffer(&nav).is_none());
        assert!(hub.state.network(&nav.network).is_none());
    });
    // Nor one that arrives after the network stopped.
    hub.update(cx, |hub, cx| hub.ingest(late, at(3), cx));
    hub.read_with(cx, |hub, _| assert!(hub.state.buffer(&lyra).is_none()));
}
