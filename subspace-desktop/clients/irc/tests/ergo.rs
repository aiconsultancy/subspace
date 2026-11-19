//! End-to-end checks against a local Ergo server. Runs only when
//! `IRC_TEST_SERVER` (`host:port`) is set; start one with
//! `scripts/irc-test-server.sh`.

use std::time::{Duration, Instant};

use chrono::Utc;
use subspace_client_irc::{
    BufferKey, Command, ConnectionStatus, Event, Line, LineKind, NetworkConfig, NetworkId, Runtime,
    Sasl, Target, Typing,
};

#[path = "ergo/anchors.rs"]
mod anchors;
#[path = "ergo/atomic.rs"]
mod atomic;

#[path = "ergo/metadata.rs"]
mod metadata;
#[path = "ergo/support.rs"]
mod support;

const TIMEOUT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(50);
const PASSWORD: &str = "subspace-test-password";

struct Client {
    runtime: Runtime,
    config: NetworkConfig,
    events: Vec<Event>,
}

impl Client {
    fn new(config: NetworkConfig) -> Self {
        let runtime = Runtime::start();
        runtime.send(Command::Configure(vec![config.clone()]));
        Self {
            runtime,
            config,
            events: Vec::new(),
        }
    }

    fn network(&self) -> NetworkId {
        self.config.id.clone()
    }

    fn buffer(&self, target: Target) -> BufferKey {
        BufferKey {
            network: self.network(),
            target,
        }
    }

    fn input(&self, target: Target, text: &str) {
        self.runtime.send(Command::Input {
            buffer: self.buffer(target),
            text: text.into(),
        });
    }

    fn mark(&mut self) -> usize {
        self.events.extend(self.runtime.drain());
        self.events.len()
    }

    /// The first event at or after `from` that matches.
    fn wait(&mut self, from: usize, what: &str, matches: impl Fn(&Event) -> bool) -> Event {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(event) = self.events.iter().skip(from).find(|event| matches(event)) {
                return event.clone();
            }
            assert!(
                Instant::now() < deadline,
                "{}: timed out waiting for {what}; events: {:#?}",
                self.config.nickname,
                &self.events[from..]
            );
            std::thread::sleep(POLL);
            self.events.extend(self.runtime.drain());
        }
    }

    fn wait_line(&mut self, from: usize, what: &str, matches: impl Fn(&Line) -> bool) -> Line {
        match self.wait(
            from,
            what,
            |event| matches!(event, Event::Line(line) if matches(line)),
        ) {
            Event::Line(line) => line,
            _ => unreachable!(),
        }
    }

    fn wait_connected(&mut self, from: usize) {
        self.wait(from, "connected", |event| {
            matches!(
                event,
                Event::Status {
                    status: ConnectionStatus::Connected { .. },
                    ..
                }
            )
        });
    }
}

/// A number for this run's nicks and channels, so runs do not collide.
use support::{config, server, suffix};

/// Registers the nick's account with NickServ, then reconnects with SASL.
fn registered_client(id: &str, nick: &str, server: &(String, u16)) -> Client {
    let mut client = Client::new(config(id, nick, server));
    client.wait_connected(0);
    let from = client.mark();
    client.input(
        Target::Server,
        &format!("/msg NickServ REGISTER {PASSWORD}"),
    );
    client.wait_line(from, "NickServ registration", |line| {
        line.text.contains("Account created") || line.text.contains("already")
    });
    let mut sasl = config(id, nick, server);
    sasl.sasl = Some(Sasl {
        username: nick.into(),
        password: PASSWORD.into(),
        password_file: None,
        external: false,
    });
    client.config = sasl.clone();
    let from = client.mark();
    client.runtime.send(Command::Configure(vec![sasl]));
    client.wait(from, "SASL capability", |event| {
        matches!(event, Event::Capabilities { enabled, .. } if enabled.iter().any(|cap| cap == "sasl"))
    });
    client.wait_line(from, "SASL success", |line| {
        line.text.contains("logged in as")
    });
    client.wait_connected(from);
    client
}

fn join(client: &mut Client, channel: &str) -> usize {
    let from = client.mark();
    client.runtime.send(Command::Join {
        network: client.network(),
        channel: channel.into(),
        key: None,
    });
    from
}

fn send(client: &Client, command: impl FnOnce(BufferKey) -> Command, target: &Target) {
    client.runtime.send(command(client.buffer(target.clone())));
}

/// Both clients join; A sees B's account through `extended-join`.
fn join_both(a: &mut Client, b: &mut Client, channel: &str) {
    let from_a = join(a, channel);
    a.wait(from_a, "A joined", |event| {
        matches!(event, Event::Joined { .. })
    });
    a.wait(from_a, "A members", |event| {
        matches!(event, Event::Members { .. })
    });
    let from_a = a.mark();
    let from_b = join(b, channel);
    b.wait(from_b, "B history on join", |event| {
        matches!(event, Event::History { .. })
    });
    let joined = a.wait(from_a, "B joins A's channel", |event| {
        matches!(event, Event::MemberJoined { .. })
    });
    let Event::MemberJoined { member, .. } = joined else {
        unreachable!()
    };
    assert_eq!(
        member.account.as_deref(),
        Some(b.config.nickname.as_str()),
        "extended-join account"
    );
}

/// echo-message, msgid, account-tag, server-time and highlights; returns
/// the received line.
fn exchange(a: &mut Client, b: &mut Client, here: &Target) -> Line {
    let (from_a, from_b) = (a.mark(), b.mark());
    a.input(here.clone(), &format!("hello {}", b.config.nickname));
    let echo = a.wait_line(from_a, "echo-message", |line| {
        line.text.starts_with("hello")
    });
    assert!(echo.own);
    assert!(echo.id.is_some(), "msgid on the echo");
    let received = b.wait_line(from_b, "A's message", |line| line.text.starts_with("hello"));
    assert_eq!(received.from.as_deref(), Some(a.config.nickname.as_str()));
    assert_eq!(
        received.account.as_deref(),
        Some(a.config.nickname.as_str()),
        "account-tag"
    );
    assert_eq!(received.id, echo.id);
    assert!(received.highlight);
    assert!(
        (Utc::now() - received.time).num_seconds().abs() < 60,
        "server-time"
    );

    let from_b = b.mark();
    a.input(here.clone(), "first line\nsecond line");
    let multiline = b.wait_line(from_b, "multiline", |line| {
        line.text.starts_with("first line")
    });
    assert_eq!(multiline.text, "first line\nsecond line");
    received
}

/// `+typing`, `MARKREAD` and a `CHATHISTORY BEFORE` page.
fn typing_markers_and_history(a: &mut Client, b: &mut Client, here: &Target, read: Line) {
    let from_a = a.mark();
    send(
        b,
        |buffer| Command::Typing {
            buffer,
            state: Typing::Active,
        },
        here,
    );
    let nick_b = b.config.nickname.clone();
    a.wait(from_a, "typing", |event| {
        matches!(event, Event::Typing { nick, state: Typing::Active, .. } if *nick == nick_b)
    });

    let from_b = b.mark();
    send(
        b,
        |buffer| Command::MarkRead {
            buffer,
            time: read.time,
        },
        here,
    );
    b.wait(from_b, "read marker", |event| {
        matches!(event, Event::ReadMarker { .. })
    });

    let from_b = b.mark();
    let before = Some(Utc::now() + chrono::Duration::seconds(5));
    send(
        b,
        |buffer| Command::LoadHistory {
            buffer,
            before,
            limit: 50,
        },
        here,
    );
    let history = b.wait(from_b, "history page", |event| {
        matches!(event, Event::History { .. })
    });
    let Event::History { lines, .. } = history else {
        unreachable!()
    };
    let texts: Vec<&str> = lines.iter().map(|line| line.text.as_str()).collect();
    assert!(
        texts.iter().any(|text| text.starts_with("hello")),
        "history: {texts:?}"
    );
    assert!(
        texts.contains(&"first line\nsecond line"),
        "history: {texts:?}"
    );
}

#[test]
fn two_clients_talk_through_ergo() {
    let Some(server) = server() else {
        eprintln!("IRC_TEST_SERVER is not set; skipping the Ergo test");
        return;
    };
    let suffix = suffix();
    let channel = format!("#subspace-{suffix}");
    let here = Target::Channel(channel.clone());
    let mut a = registered_client("a", &format!("ssa{suffix}"), &server);
    let mut b = registered_client("b", &format!("ssb{suffix}"), &server);

    join_both(&mut a, &mut b, &channel);
    let received = exchange(&mut a, &mut b, &here);
    typing_markers_and_history(&mut a, &mut b, &here, received);

    let (from_a, from_b) = (a.mark(), b.mark());
    b.runtime.send(Command::Disconnect {
        network: b.network(),
        reason: "done".into(),
    });
    a.wait(from_a, "B quits", |event| {
        matches!(event, Event::MemberLeft { .. })
    });
    b.wait(from_b, "B disconnected", |event| {
        matches!(
            event,
            Event::Status {
                status: ConnectionStatus::Disconnected,
                ..
            }
        )
    });
}
