use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpListener;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use subspace_core::config::Config;
use subspace_core::copilot_agent_tasks::{Submission, SubmissionTimeout};
use subspace_core::credentials::{Credentials, Environment, Runner};

use crate::Client;

struct TestEnv;
impl Environment for TestEnv {
    fn var(&self, name: &str) -> Option<String> {
        (name == "GH_TOKEN").then(|| "test-only-offline-credential".into())
    }
}

struct NoCli;
impl Runner for NoCli {
    fn run(&self, _: &str, _: &[&str], _: &[&str], _: Duration) -> Result<String, String> {
        Err("No CLI in this fixture".into())
    }
}

fn blocked_post(listener: TcpListener, entered: mpsc::Sender<()>, release: mpsc::Receiver<()>) {
    let (mut stream, _) = listener.accept().unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert!(line.starts_with("POST /agents/repos/"));
    let mut length = 0;
    loop {
        line.clear();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap();
        }
    }
    reader.read_exact(&mut vec![0; length]).unwrap();
    entered.send(()).unwrap();
    release.recv().unwrap();
    let body = r#"{"id":"test-task-posted","state":"queued"}"#;
    // A correct timed-out client has already closed the connection.
    _ = write!(
        stream,
        "HTTP/1.1 201 Created\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

#[test]
fn an_inflight_post_uses_the_remaining_budget_and_keeps_an_unknown_outcome() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (entered, observed) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    let server = std::thread::spawn(move || blocked_post(listener, entered, blocked));
    let credentials = Credentials::new(&Config::default(), TestEnv, NoCli);
    let client = Client::new(Arc::new(credentials)).with_api_url(url);
    let submission = Submission::new(Instant::now() + Duration::from_secs(2));
    let attempt = submission.clone();
    let (finished, result) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let posted = super::post(
            &client,
            "test-user",
            "/agents/repos/example/test-only/tasks",
            serde_json::json!({"prompt":"test-only prompt"}),
            &attempt,
        );
        finished.send(posted).unwrap();
    });
    observed.recv_timeout(Duration::from_secs(3)).unwrap();
    let received = result.recv_timeout(Duration::from_secs(5));
    release.send(()).unwrap();
    worker.join().unwrap();
    server.join().unwrap();
    assert!(
        matches!(received, Ok(Err(_))),
        "POST must stop waiting before the held server response: {received:?}"
    );
    assert_eq!(submission.expire(), SubmissionTimeout::Unknown);
    assert_eq!(submission.begin_dispatch(), Err(SubmissionTimeout::Unknown));
}
