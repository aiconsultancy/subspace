//! A tool's message arrives in a real channel. Runs only when
//! `IRC_TEST_SERVER` (`host:port`) is set; start a server with
//! `scripts/irc-test-server.sh`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use chrono::Utc;
use serde_json::{Value, json};
use subspace_client_irc::{Command, Event, NetworkConfig, NetworkId, Runtime, Target};
use subspace_framework::agent_tools::{AgentTool, Invocation, Unattended, Unmonitored};

use super::super::bridge::Link;
use super::super::{serve, tools};
use crate::state::IrcState;

const TIMEOUT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(50);

fn server() -> Option<(String, u16)> {
    let value = std::env::var("IRC_TEST_SERVER").ok()?;
    let (host, port) = value.rsplit_once(':')?;
    Some((host.to_owned(), port.parse().ok()?))
}

fn network(id: &str, nick: &str, channel: &str, (host, port): &(String, u16)) -> NetworkConfig {
    serde_json::from_value(json!({
        "id": id, "name": id, "host": host, "port": port, "tls": false,
        "nickname": nick, "channels": [channel]
    }))
    .unwrap()
}

/// The hub's drain loop on a thread: events into the state, tool calls
/// answered from it, their commands sent through the same runtime.
fn hub(config: NetworkConfig, stop: Arc<AtomicBool>) -> (Link, JoinHandle<()>) {
    let (sender, requests) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        let runtime = Runtime::start();
        runtime.send(Command::Configure(vec![config.clone()]));
        let mut state = IrcState::new(&[config], Vec::new());
        while !stop.load(Ordering::Relaxed) {
            for event in runtime.drain() {
                state.apply(event, Utc::now());
            }
            for request in requests.try_iter() {
                serve(&mut state, request, &super::people)
                    .into_iter()
                    .for_each(|command| runtime.send(command));
            }
            std::thread::sleep(POLL);
        }
    });
    (Link::Fixed(sender), handle)
}

fn call(tool: &Arc<dyn AgentTool>, arguments: Value) -> Result<Value, String> {
    let runtime = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
    runtime.block_on(tool.call(Invocation {
        arguments,
        approval: Arc::new(Unattended),
        monitor: Arc::new(Unmonitored),
    }))
}

fn until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(POLL);
    }
}

/// A plain client in `channel` that records what it hears.
struct Listener {
    runtime: Runtime,
    heard: Vec<Event>,
}

impl Listener {
    fn join(nick: &str, channel: &str, server: &(String, u16)) -> Self {
        let runtime = Runtime::start();
        runtime.send(Command::Configure(vec![network(
            "b", nick, channel, server,
        )]));
        let mut listener = Self {
            runtime,
            heard: Vec::new(),
        };
        listener.wait("the listener to join", |event| {
            matches!(event, Event::Joined { .. })
        });
        listener
    }

    fn wait(&mut self, what: &str, matches: impl Fn(&Event) -> bool) {
        until(what, || {
            self.heard.extend(self.runtime.drain());
            self.heard.iter().any(&matches)
        });
    }
}

/// Waits until `read` returns a reply that `accept` takes.
fn until_read(
    tools: &[Arc<dyn AgentTool>],
    what: &str,
    read: Value,
    accept: impl Fn(&Value) -> bool,
) {
    until(what, || {
        call(&tools[0], read.clone()).is_ok_and(|value| accept(&value))
    });
}

fn any_line(value: &Value, key: &str, accept: impl Fn(&Value) -> bool) -> bool {
    value[key]
        .as_array()
        .is_some_and(|items| items.iter().any(accept))
}

#[test]
fn a_tool_message_arrives_in_the_channel_under_the_users_nick() {
    let Some(server) = server() else {
        eprintln!("IRC_TEST_SERVER is not set; skipping the Ergo tool test");
        return;
    };
    let unique = Utc::now().timestamp_millis() % 1_000_000;
    let (channel, nick) = (format!("#tools-{unique}"), format!("sst{unique}"));
    let stop = Arc::new(AtomicBool::new(false));
    let (link, hub) = hub(network("a", &nick, &channel, &server), stop.clone());
    let tools = tools(link, None);
    let mut listener = Listener::join(&format!("ssl{unique}"), &channel, &server);
    until_read(
        &tools,
        "the tool's network to join",
        json!({"action":"buffers","network":"a"}),
        |value| any_line(value, "buffers", |buffer| buffer["joined"] == true),
    );

    let text = "hello from a tool";
    let message = json!({"action":"message","network":"a","target":channel,"text":text});
    let sent = call(&tools[1], json!({ "operations": [message] })).unwrap();
    assert_eq!(sent["results"][0]["success"], true, "{sent}");
    listener.wait("the message", |event| {
        matches!(event, Event::Line(line) if line.text == text
            && line.from.as_deref() == Some(nick.as_str())
            && line.buffer.target == Target::Channel(channel.clone()))
    });
    let messages = json!({"action":"messages","network":"a","target":channel});
    until_read(
        &tools,
        "the echo in the tool's own buffer",
        messages,
        |value| {
            any_line(value, "lines", |line| {
                line["own"] == true && line["text"] == text
            })
        },
    );

    listener.runtime.send(Command::Disconnect {
        network: NetworkId("b".into()),
        reason: String::new(),
    });
    drop(tools);
    stop.store(true, Ordering::Relaxed);
    assert!(hub.join().is_ok());
}
