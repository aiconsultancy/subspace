use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};
use subspace_core::config::Config;
use subspace_core::credentials::{Credentials, Environment, Runner};

use crate::Client;
use crate::copilot_agents::Task;
use crate::copilot_agents::map::linked_task;

struct Env;
impl Environment for Env {
    fn var(&self, name: &str) -> Option<String> {
        (name == "GH_TOKEN").then(|| "fixture-token".into())
    }
}

struct NoRunner;
impl Runner for NoRunner {
    fn run(&self, _: &str, _: &[&str], _: &[&str], _: Duration) -> Result<String, String> {
        Err("unused".into())
    }
}

fn server(responses: Vec<(u16, Value)>) -> (Client, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let requests = seen.clone();
    thread::spawn(move || {
        for ((status, body), stream) in responses.into_iter().zip(listener.incoming()) {
            let mut stream = stream.unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            requests.lock().unwrap().push(line.trim_end().to_owned());
            loop {
                line.clear();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
            }
            let body = body.to_string();
            write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    let credentials = Credentials::new(&Config::default(), Env, NoRunner);
    (Client::new(Arc::new(credentials)).with_api_url(url), seen)
}

fn raw(id: &str) -> Value {
    json!({
        "id": id, "state": "completed", "repository": {"id": 4242},
        "artifacts": [{"type": "pull", "data": {"id": 42, "global_id": ""}}],
        "sessions": [{"id": "session-1"}]
    })
}

fn resolve_first(client: &Client) -> Task {
    let mut rows = [raw("task-1"), raw("task-2")];
    let pulls = super::resolve_task_pulls(client, "work_emu", &mut rows);
    linked_task(&rows[0], None, &pulls)
}

#[test]
fn missing_node_ids_resolve_once_per_repository_and_produce_session_links() {
    let (client, seen) = server(vec![
        (200, json!({"full_name": "Owner/one"})),
        (
            200,
            json!([{"id": 42, "number": 7,
            "html_url": "https://github.com/Owner/one/pull/7",
            "state": "open", "draft": true, "merged_at": null}]),
        ),
    ]);
    let task = resolve_first(&client);
    assert_eq!(task.repo, "Owner/one");
    assert_eq!(task.pull_request.as_ref().map(|pull| pull.number), Some(7));
    assert_eq!(
        task.html_url,
        "https://github.com/Owner/one/pull/7/agent-sessions/session-1"
    );
    assert_eq!(
        *seen.lock().unwrap(),
        [
            "GET /repositories/4242 HTTP/1.1",
            "GET /repos/Owner/one/pulls?state=all&sort=created&direction=desc&per_page=100 HTTP/1.1",
        ]
    );
}

#[test]
fn failed_optional_repository_lookup_keeps_the_task_without_inventing_a_route() {
    let (client, seen) = server(vec![(403, json!({"message": "forbidden"}))]);
    let task = resolve_first(&client);
    assert_eq!(task.id, "task-1");
    assert!(task.html_url.is_empty());
    assert!(task.repo.is_empty());
    assert_eq!(seen.lock().unwrap().len(), 1);
}
