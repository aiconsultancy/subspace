use super::{Client, Policy, SearchProvider};
use crate::fetch::{BODY_LIMIT, Backend, Response};
use crate::safety::{addresses_are_public, blocked_ip, host_allowed};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::Duration;
use url::Url;

#[derive(Debug)]
struct FixtureServer(Mutex<VecDeque<Result<Response, String>>>);

impl FixtureServer {
    fn new(responses: Vec<Result<Response, String>>) -> Self {
        Self(Mutex::new(responses.into()))
    }
}

impl Backend for FixtureServer {
    fn get(&self, _: &Url, _: Duration) -> Result<Response, String> {
        self.0.lock().unwrap().pop_front().unwrap()
    }
}

fn response(content_type: &str, body: impl Into<Vec<u8>>) -> Result<Response, String> {
    Ok(Response {
        location: None,
        content_type: content_type.into(),
        body: body.into(),
    })
}

fn policy() -> Policy {
    Policy {
        allowed_hosts: vec!["example.com".into(), "*.example.org".into()],
        silent_hosts: vec![],
    }
}

#[test]
fn ssrf_address_table_is_blocked() {
    for address in [
        "127.0.0.1",
        "10.0.0.1",
        "172.16.0.1",
        "192.168.0.1",
        "169.254.1.1",
        "169.254.169.254",
        "100.64.0.1",
        "0.0.0.0",
        "224.0.0.1",
        "::",
        "::1",
        "fc00::1",
        "fe80::1",
        "ff02::1",
    ] {
        assert!(blocked_ip(address.parse::<IpAddr>().unwrap()), "{address}");
    }
    assert!(!blocked_ip("93.184.216.34".parse().unwrap()));
    assert!(
        !addresses_are_public([
            "93.184.216.34".parse().unwrap(),
            "127.0.0.1".parse().unwrap(),
        ]),
        "a rebinding answer containing one blocked address is refused"
    );
}

#[test]
fn allow_list_wildcard_matches_exactly_one_leading_label() {
    assert!(host_allowed("example.com", &policy().allowed_hosts));
    assert!(host_allowed("docs.example.org", &policy().allowed_hosts));
    assert!(!host_allowed("example.org", &policy().allowed_hosts));
    assert!(!host_allowed("a.b.example.org", &policy().allowed_hosts));
}

#[test]
fn redirects_are_revalidated_and_bounded() {
    let redirects = (0..4)
        .map(|_| {
            Ok(Response {
                location: Some("https://example.com/next".into()),
                content_type: String::new(),
                body: vec![],
            })
        })
        .collect();
    let client = Client::with_backend(policy(), FixtureServer::new(redirects));
    assert!(
        client
            .read("https://example.com/start")
            .unwrap_err()
            .contains("three redirects")
    );

    let client = Client::with_backend(
        policy(),
        FixtureServer::new(vec![Ok(Response {
            location: Some("https://blocked.invalid/".into()),
            content_type: String::new(),
            body: vec![],
        })]),
    );
    assert!(
        client
            .read("https://example.com/")
            .unwrap_err()
            .contains("not allowed")
    );
}

#[test]
fn body_content_and_active_content_are_bounded() {
    let body = vec![b'a'; BODY_LIMIT + 1];
    let client = Client::with_backend(
        policy(),
        FixtureServer::new(vec![response("text/plain", body)]),
    );
    let result = client.read("https://example.com/").unwrap();
    assert!(result.truncated);
    assert!(result.text.ends_with("[truncated at 1 MiB]"));

    let client = Client::with_backend(
        policy(),
        FixtureServer::new(vec![response(
            "text/html; charset=utf-8",
            "<h1>Evidence</h1><script>steal()</script><style>x{}</style><p>safe</p>",
        )]),
    );
    let result = client.read("https://example.com/").unwrap();
    assert!(result.text.contains("Evidence"));
    assert!(result.text.contains("safe"));
    assert!(!result.text.contains("steal"));
}

#[test]
fn envelope_content_type_deadline_and_rate_limits_are_enforced() {
    let client = Client::with_backend(
        policy(),
        FixtureServer::new(vec![response("image/png", vec![])]),
    );
    assert!(
        client
            .read("https://example.com/")
            .unwrap_err()
            .contains("content type")
    );

    let responses = (0..11)
        .map(|_| response("application/json", "{}"))
        .collect();
    let client = Client::with_backend(policy(), FixtureServer::new(responses));
    for _ in 0..10 {
        let result = client.read("https://example.com/").unwrap();
        assert!(result.untrusted);
        assert_eq!(result.source, "web");
        assert_eq!(result.final_url, "https://example.com/");
    }
    assert!(
        client
            .read("https://example.com/")
            .unwrap_err()
            .contains("rate limit")
    );

    let client = Client::with_backend(
        policy(),
        FixtureServer::new(vec![Err("web_read exceeded its deadline".into())]),
    );
    assert!(
        client
            .read("https://example.com/")
            .unwrap_err()
            .contains("deadline")
    );
}

#[test]
fn invalid_urls_are_refused_before_the_fixture_server() {
    let client = Client::with_backend(policy(), FixtureServer::new(vec![]));
    for url in [
        "http://example.com",
        "https://user@example.com",
        "https://blocked.invalid",
    ] {
        assert!(client.read(url).is_err(), "{url}");
    }
}

struct FixtureProvider;

impl SearchProvider for FixtureProvider {
    fn id(&self) -> &str {
        "fixture"
    }
    fn search(&self, query: &str) -> Result<Value, String> {
        Ok(json!({"received": query}))
    }
}

#[test]
fn provider_receives_the_query_only() {
    let provider = FixtureProvider;
    assert_eq!(provider.id(), "fixture");
    assert_eq!(
        provider.search("terms").unwrap(),
        json!({"received":"terms"})
    );
}
