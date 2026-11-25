//! The one event stream as it subscribes to officers, against raw
//! loopback servers.

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use subspace_framework::core::Secret;

use super::{TOKEN, eventually, quick};
use crate::feed::{Feed, Link};
use crate::{FleetAddress, FleetClient};

/// A listener on a free loopback port and its address.
fn loopback() -> (TcpListener, FleetAddress) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let address = FleetAddress::parse(&format!("http://127.0.0.1:{port}")).unwrap();
    (listener, address)
}

/// Reads a request's head from `stream` and returns its target.
fn request_target(stream: &TcpStream) -> String {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut first = String::new();
    reader.read_line(&mut first).unwrap();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
    }
    first.split(' ').nth(1).unwrap_or_default().to_owned()
}

/// A loopback stream server that keeps every stream open, records each
/// request target and sends one `agent-activity` event on a stream that
/// subscribed to `example-lyra`.
fn hold_streams() -> (FleetAddress, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    let (listener, address) = loopback();
    let targets = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = std::sync::Arc::clone(&targets);
    std::thread::spawn(move || {
        let mut open = Vec::new();
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let target = request_target(&stream);
            let mut reply = String::from(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n\
                 event: heartbeat\ndata: {\"at\":1}\n\n",
            );
            if target.contains("agent=example-lyra") {
                reply.push_str(concat!(
                    "event: agent-activity\ndata: ",
                    r#"{"agentId":"example-lyra","turnId":"t1","generation":"g1","seq":7,"at":1,"change":{"type":"item","item":{"id":"i1","seq":7,"startedAt":1,"updatedAt":1,"kind":"text","status":"streaming","text":"hi"}}}"#,
                    "\n\n"
                ));
            }
            seen.lock().unwrap().push(target);
            let _ = stream.write_all(reply.as_bytes());
            open.push(stream);
        }
    });
    (address, targets)
}

#[test]
fn the_one_stream_reopens_with_the_watched_officer_and_without_it_after() {
    let (address, targets) = hold_streams();
    let feed = Feed::start_with(FleetClient::new(address, Secret::new(TOKEN)), quick());
    assert!(eventually(|| feed.link() == Link::Live));
    assert!(feed.activity_news("example-lyra", 0).is_none());
    feed.set_agents(["example-lyra".to_owned()].into());
    assert!(eventually(|| {
        feed.activity_news("example-lyra", 0)
            .is_some_and(|news| news.connection.is_some() && news.events.len() == 1)
    }));
    let news = feed.activity_news("example-lyra", 0).unwrap();
    assert_eq!(news.events[0].seq, 7);
    feed.set_agents(std::collections::BTreeSet::new());
    assert!(eventually(|| targets.lock().unwrap().len() == 3));
    assert_eq!(
        *targets.lock().unwrap(),
        [
            "/api/v1/events",
            "/api/v1/events?agent=example-lyra",
            "/api/v1/events"
        ]
    );
    assert!(feed.activity_news("example-lyra", 0).is_none());
}

#[test]
fn a_stream_that_refuses_the_officer_opens_again_without_it() {
    let (listener, address) = loopback();
    let targets = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = std::sync::Arc::clone(&targets);
    std::thread::spawn(move || {
        let mut open = Vec::new();
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let target = request_target(&stream);
            let reply = if target.contains("agent=") {
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            } else {
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n"
            };
            seen.lock().unwrap().push(target);
            let _ = stream.write_all(reply.as_bytes());
            open.push(stream);
        }
    });
    let feed = Feed::start_with(FleetClient::new(address, Secret::new(TOKEN)), quick());
    feed.set_agents(["example-gone".to_owned()].into());
    assert!(eventually(|| {
        feed.activity_news("example-gone", 0)
            .is_some_and(|news| news.refused)
            && feed.link() == Link::Live
    }));
    assert_eq!(
        targets.lock().unwrap().last().map(String::as_str),
        Some("/api/v1/events")
    );
}

/// A stream asked for `example-a` is refused late, after the watch moved
/// to `example-b`: the refusal is about `example-a` only, so the stream
/// asks for `example-b` next and carries it live.
#[test]
fn a_late_refusal_never_refuses_the_officer_watched_since() {
    let (listener, address) = loopback();
    let targets = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = std::sync::Arc::clone(&targets);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let seen = std::sync::Arc::clone(&seen);
            std::thread::spawn(move || {
                let target = request_target(&stream);
                seen.lock().unwrap().push(target.clone());
                if target.contains("agent=example-a") {
                    std::thread::sleep(Duration::from_millis(500));
                    let _ = stream.write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                    return;
                }
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n\
                      event: heartbeat\ndata: {\"at\":1}\n\n",
                );
                std::thread::sleep(Duration::from_secs(30));
            });
        }
    });
    let feed = Feed::start_with(FleetClient::new(address, Secret::new(TOKEN)), quick());
    feed.set_agents(["example-a".to_owned()].into());
    assert!(eventually(|| {
        targets
            .lock()
            .unwrap()
            .iter()
            .any(|target| target.contains("agent=example-a"))
    }));
    // Replaced while the stream that asked for A still waits on its answer.
    feed.set_agents(["example-b".to_owned()].into());
    assert!(eventually(|| {
        feed.activity_news("example-b", 0)
            .is_some_and(|news| news.connection.is_some() && !news.refused)
    }));
    assert!(
        targets
            .lock()
            .unwrap()
            .iter()
            .any(|target| target == "/api/v1/events?agent=example-b"),
        "{:?}",
        targets.lock().unwrap()
    );
    assert_eq!(feed.link(), Link::Live);
}

/// A loopback server whose streams that subscribe to an officer hold back
/// their headers for a while and close soon after sending them, so the
/// feed reconnects. It records, in order, each such stream's headers as
/// they go out and each snapshot request.
fn slow_officer_streams() -> (FleetAddress, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    let (listener, address) = loopback();
    let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = std::sync::Arc::clone(&log);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let seen = std::sync::Arc::clone(&seen);
            std::thread::spawn(move || {
                let target = request_target(&stream);
                let headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n\
                               event: heartbeat\ndata: {\"at\":1}\n\n";
                if target.starts_with("/api/v1/agents/") {
                    seen.lock().unwrap().push("snapshot".to_owned());
                    let body = r#"{"agent":{"id":"example-lyra","role":"captain","backend":"opencode","collected":true},"turns":[],"cursor":{"generation":"g1","seq":0}}"#;
                    let _ = stream.write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                        .as_bytes(),
                    );
                } else if target.contains("agent=") {
                    std::thread::sleep(Duration::from_millis(300));
                    seen.lock().unwrap().push("headers".to_owned());
                    let _ = stream.write_all(headers.as_bytes());
                    std::thread::sleep(Duration::from_millis(300));
                } else {
                    let _ = stream.write_all(headers.as_bytes());
                    std::thread::sleep(Duration::from_secs(30));
                }
            });
        }
    });
    (address, log)
}

#[test]
fn the_snapshot_waits_for_the_officer_streams_headers_at_first_and_on_reconnect() {
    use crate::FleetState;
    use crate::activity::ActivityJoin;

    let (address, log) = slow_officer_streams();
    let client = FleetClient::new(address, Secret::new(TOKEN));
    let feed = Feed::start_with(client.clone(), quick());
    feed.set_agents(["example-lyra".to_owned()].into());
    let mut join = ActivityJoin::new();
    let mut fetches = 0;
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    // The view's loop: the fleet's state and news, then any wanted fetch.
    while fetches < 2 && std::time::Instant::now() < deadline {
        join.set_fleet_state(&FleetState::Followed(feed.link()));
        if let Some(news) = feed.activity_news("example-lyra", join.next()) {
            join.absorb(news);
        }
        if join.start_fetch() {
            fetches += 1;
            join.finish_fetch(client.activity("example-lyra", None));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(fetches, 2, "a snapshot at first and after the reconnect");
    let log = log.lock().unwrap().clone();
    let mut headers = 0;
    let mut snapshots = 0;
    for entry in &log {
        match entry.as_str() {
            "headers" => headers += 1,
            _ => {
                snapshots += 1;
                assert!(
                    headers >= snapshots,
                    "snapshot {snapshots} before its stream answered: {log:?}"
                );
            }
        }
    }
    assert_eq!(snapshots, 2, "{log:?}");
}
