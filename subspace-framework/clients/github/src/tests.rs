//! Client tests against a local HTTP server with synthetic answers.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::json;
use subspace_core::config::Config;
use subspace_core::credentials::{Credentials, Environment, Runner};
use subspace_core::http::Error;

use crate::Client;
use crate::model::{PrState, ReviewEvent};

mod bound;
mod copilot_and_assignable;
mod diffs;
mod lifecycle;
mod package_errors;
mod package_reads;
mod repositories;
mod request_budget;
mod work_account;
mod work_pin;

struct Env(Arc<Mutex<String>>);

impl Environment for Env {
    fn var(&self, name: &str) -> Option<String> {
        (name == "GH_TOKEN").then(|| self.0.lock().unwrap().clone())
    }
}

struct NoCli;

impl Runner for NoCli {
    fn run(&self, _: &str, _: &[&str], _: &[&str], _: Duration) -> Result<String, String> {
        Err("no cli in tests".into())
    }
}

pub(super) struct Request {
    line: String,
    authorization: String,
    accept: String,
    body: String,
}

pub(super) type Handler = dyn Fn(&Request) -> (u16, String) + Send + Sync;

/// Serves one response per connection until the test ends.
pub(super) fn serve(handler: Arc<Handler>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let handler = handler.clone();
            thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let (mut length, mut authorization, mut accept) = (0, String::new(), String::new());
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).unwrap();
                    let header = header.trim_end();
                    if header.is_empty() {
                        break;
                    }
                    let (name, value) = header.split_once(": ").unwrap_or((header, ""));
                    match name.to_ascii_lowercase().as_str() {
                        "content-length" => length = value.parse().unwrap(),
                        "authorization" => authorization = value.to_owned(),
                        "accept" => accept = value.to_owned(),
                        _ => {}
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let request = Request {
                    line: line.trim_end().to_owned(),
                    authorization,
                    accept,
                    body: String::from_utf8(body).unwrap(),
                };
                let (status, body) = handler(&request);
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            });
        }
    });
    format!("http://{address}")
}

pub(super) fn client(url: String, token: &Arc<Mutex<String>>) -> Client {
    let credentials = Credentials::new(&Config::default(), Env(token.clone()), NoCli);
    Client::new(Arc::new(credentials)).with_api_url(url)
}

pub(super) fn token(value: &str) -> Arc<Mutex<String>> {
    Arc::new(Mutex::new(value.to_owned()))
}

pub(super) fn pr(id: &str, number: u32) -> serde_json::Value {
    json!({
        "__typename": "PullRequest", "id": id, "number": number, "title": "t",
        "url": format!("https://github.com/o/r/pull/{number}"), "state": "OPEN",
        "createdAt": "2026-09-01T00:00:00Z", "updatedAt": "2026-09-02T00:00:00Z",
        "repository": {"nameWithOwner": "o/r"}, "author": {"login": "ann"},
        "assignees": {"nodes": []}, "labels": {"nodes": []}, "comments": {"totalCount": 0},
        "isDraft": false, "commits": {"nodes": []}
    })
}

#[test]
fn a_pr_section_merges_three_searches() {
    let handler: Arc<Handler> = Arc::new(|request: &Request| {
        assert_eq!(request.line, "POST /graphql HTTP/1.1");
        assert_eq!(request.authorization, "Bearer tok");
        let body: serde_json::Value = serde_json::from_str(&request.body).unwrap();
        let query = body["query"].as_str().unwrap();
        assert_eq!(
            body["variables"]["query"],
            "is:pr is:open review-requested:@me archived:false sort:updated-desc"
        );
        let nodes = if query.contains("reviewDecision") {
            json!([{"__typename": "PullRequest", "id": "A", "reviewDecision": "APPROVED"}])
        } else if query.contains("additions") {
            return (502, "{}".into());
        } else {
            json!([pr("A", 1), null])
        };
        let answer = json!({
            "data": {"search": {"issueCount": 9, "nodes": nodes}},
            "errors": [{"type": "FORBIDDEN", "message": "SAML enforcement"}]
        });
        (200, answer.to_string())
    });
    let client = client(serve(handler), &token("tok"));
    let section = client.section("ann", "review", None).unwrap();
    assert_eq!(section.total, 9);
    assert_eq!(section.items.len(), 1);
    let item = &section.items[0];
    assert_eq!(item.state, PrState::Open);
    assert_eq!(
        item.review_decision,
        Some(crate::model::ReviewDecision::Approved)
    );
    assert_eq!(item.additions, None);
}

#[test]
fn a_401_retries_once_with_a_fresh_token() {
    let calls = Arc::new(AtomicUsize::new(0));
    let token_value = token("old");
    let (seen, swap) = (calls.clone(), token_value.clone());
    let handler: Arc<Handler> = Arc::new(move |request: &Request| {
        seen.fetch_add(1, Ordering::SeqCst);
        if request.authorization == "Bearer old" {
            // The user fixes GH_TOKEN before the retry reads it again.
            *swap.lock().unwrap() = "new".into();
            return (401, r#"{"message": "Bad credentials"}"#.into());
        }
        (
            200,
            json!({"data": {"viewer": {"login": "ann"}}}).to_string(),
        )
    });
    let client = client(serve(handler), &token_value);
    assert_eq!(client.viewer_login(None).unwrap(), "ann");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn rate_limits_and_missing_items_map_to_statuses() {
    let handler: Arc<Handler> = Arc::new(|request: &Request| {
        if request.line.starts_with("GET /repos/o/r/pulls/5 ") {
            return (
                403,
                r#"{"message": "API rate limit exceeded for user"}"#.into(),
            );
        }
        (
            200,
            json!({"data": {"repository": {"issueOrPullRequest": null}}}).to_string(),
        )
    });
    let client = client(serve(handler), &token("tok"));
    let status = |error: Error| match error {
        Error::Status { status, .. } => status,
        _ => 0,
    };
    assert_eq!(status(client.diff("ann", "o/r#5").unwrap_err()), 429);
    assert_eq!(status(client.item("ann", "o/r#6").unwrap_err()), 404);
    // Validation runs before any request.
    let comment = client.comment("ann", "o/r#6", "   ").unwrap_err();
    assert_eq!(status(comment), 400);
    let review = client.review("ann", "o/r", ReviewEvent::Approve, "");
    assert_eq!(status(review.unwrap_err()), 400);
}
