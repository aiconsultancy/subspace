//! MCP over Streamable HTTP: streamed SSE responses, protocol headers,
//! Origin, Host and bearer-token checks, and a legacy client.

mod common;

#[path = "common/http.rs"]
mod client;

use client::{Http, modern_headers, send};
use common::{LEGACY, MODERN, meta, request, two_notes};
use serde_json::json;

#[path = "http/hosting.rs"]
mod hosting;

#[tokio::test]
async fn http_streams_progress_then_the_result_on_one_sse_response() {
    let server = Http::start(&[], None).await;
    let mut params = two_notes();
    params["_meta"] = meta(json!({"progressToken":"http-1"}));
    let reply = send(
        &server,
        http::Method::POST,
        &modern_headers("tools/call", Some("notes_write")),
        Some(request(1, "tools/call", params)),
    )
    .await;
    assert_eq!(reply.status, 200, "{}", reply.body());
    assert_eq!(reply.headers["content-type"], "text/event-stream");
    assert!(reply.headers.get("mcp-session-id").is_none());
    let events = reply.events();
    let methods: Vec<&str> = events
        .iter()
        .map(|event| event["method"].as_str().unwrap_or("response"))
        .collect();
    assert_eq!(
        methods,
        [
            "notifications/progress",
            "notifications/progress",
            "response"
        ]
    );
    assert_eq!(events[2]["result"]["structuredContent"]["approved"], true);
    let first_progress = reply
        .frames
        .iter()
        .position(|frame| frame.contains("notifications/progress"))
        .unwrap();
    let result = reply
        .frames
        .iter()
        .position(|frame| frame.contains("\"result\""))
        .unwrap();
    assert!(
        first_progress < result,
        "the progress arrives in an earlier frame than the result"
    );
}

#[tokio::test]
async fn http_discovery_and_listing_work_and_mismatched_headers_are_refused() {
    let server = Http::start(&[], None).await;
    let discovered = send(
        &server,
        http::Method::POST,
        &modern_headers("server/discover", None),
        Some(request(
            1,
            "server/discover",
            json!({"_meta": meta(json!({}))}),
        )),
    )
    .await;
    assert_eq!(discovered.status, 200, "{}", discovered.body());
    let result = &discovered.events()[0]["result"];
    assert!(
        result["supportedVersions"]
            .as_array()
            .unwrap()
            .contains(&json!(MODERN))
    );
    for (id, method) in [(2, "resources/list"), (3, "resources/templates/list")] {
        let listed = send(
            &server,
            http::Method::POST,
            &modern_headers(method, None),
            Some(request(id, method, json!({"_meta": meta(json!({}))}))),
        )
        .await;
        assert_eq!(listed.status, 200, "{}: {}", method, listed.body());
        let payload = &listed.events()[0]["result"];
        if method == "resources/list" {
            assert!(payload["resources"].as_array().unwrap().is_empty());
        } else {
            let templates = payload["resourceTemplates"].as_array().unwrap();
            assert!(templates.is_empty());
        }
    }
    let mismatch = send(
        &server,
        http::Method::POST,
        &modern_headers("tools/call", Some("notes_write")),
        Some(request(
            4,
            "tools/call",
            json!({"_meta": meta(json!({})), "name":"echo_read","arguments":{"action":"echo","text":"x"}}),
        )),
    )
    .await;
    assert_eq!(mismatch.status, 400, "{}", mismatch.body());
    assert!(mismatch.body().contains("-32020"), "{}", mismatch.body());
    let get = send(&server, http::Method::GET, &[], None).await;
    assert_eq!(get.status, 405);
    let delete = send(&server, http::Method::DELETE, &[], None).await;
    assert_eq!(delete.status, 405);
}

#[tokio::test]
async fn http_refuses_foreign_origins_and_hosts_but_accepts_local_ones() {
    let server = Http::start(&["--allow-origin", "https://app.example:443"], None).await;
    let list = || Some(request(1, "tools/list", json!({"_meta": meta(json!({}))})));
    let with = |name: &'static str, value: &str| {
        let mut headers = modern_headers("tools/list", None);
        headers.push((name, value.to_owned()));
        headers
    };
    let foreign = send(
        &server,
        http::Method::POST,
        &with("origin", "https://evil.example"),
        list(),
    )
    .await;
    assert_eq!(foreign.status, 403);
    for allowed in ["http://localhost:6274", "https://app.example"] {
        let reply = send(
            &server,
            http::Method::POST,
            &with("origin", allowed),
            list(),
        )
        .await;
        assert_eq!(reply.status, 200, "{allowed}: {}", reply.body());
    }
    let mut rebound = modern_headers("tools/list", None);
    rebound.push(("host", "evil.example".to_owned()));
    let stream = tokio::net::TcpStream::connect(server.address)
        .await
        .unwrap();
    let (mut sender, connection) =
        hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(stream))
            .await
            .unwrap();
    tokio::spawn(connection);
    let mut builder = http::Request::post("/mcp");
    for (name, value) in &rebound {
        builder = builder.header(*name, value);
    }
    let response = sender
        .send_request(
            builder
                .body(http_body_util::Full::new(bytes::Bytes::from(
                    list().unwrap().to_string(),
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(response.status().is_client_error(), "{}", response.status());
}

#[tokio::test]
async fn http_needs_the_bearer_token_when_one_is_set() {
    let server = Http::start(&[], Some("http-secret")).await;
    let list = || Some(request(1, "tools/list", json!({"_meta": meta(json!({}))})));
    let missing = send(
        &server,
        http::Method::POST,
        &modern_headers("tools/list", None),
        list(),
    )
    .await;
    assert_eq!(missing.status, 401);
    assert_eq!(missing.headers["www-authenticate"], "Bearer");
    let mut wrong = modern_headers("tools/list", None);
    wrong.push(("authorization", "Bearer other".to_owned()));
    assert_eq!(
        send(&server, http::Method::POST, &wrong, list())
            .await
            .status,
        401
    );
    let mut right = modern_headers("tools/list", None);
    right.push(("authorization", "Bearer http-secret".to_owned()));
    let reply = send(&server, http::Method::POST, &right, list()).await;
    assert_eq!(reply.status, 200, "{}", reply.body());
    assert_eq!(
        reply.events()[0]["result"]["tools"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn an_http_flood_receives_the_documented_rate_limit_response() {
    let server = Http::start(&[], None).await;
    let list = || Some(request(1, "tools/list", json!({"_meta": meta(json!({}))})));
    for _ in 0..60 {
        assert_eq!(
            send(
                &server,
                http::Method::POST,
                &modern_headers("tools/list", None),
                list(),
            )
            .await
            .status,
            200
        );
    }
    let limited = send(
        &server,
        http::Method::POST,
        &modern_headers("tools/list", None),
        list(),
    )
    .await;
    assert_eq!(limited.status, 429);
    assert!(limited.body().contains("60 requests per minute"));
}

#[tokio::test]
async fn a_legacy_http_client_initializes_without_a_session_and_lists() {
    let server = Http::start(&[], None).await;
    let legacy = |protocol: Option<&str>| {
        let mut headers = vec![
            ("content-type", "application/json".to_owned()),
            ("accept", "application/json, text/event-stream".to_owned()),
        ];
        if let Some(protocol) = protocol {
            headers.push(("mcp-protocol-version", protocol.to_owned()));
        }
        headers
    };
    let initialized = send(
        &server,
        http::Method::POST,
        &legacy(None),
        Some(request(
            1,
            "initialize",
            json!({"protocolVersion":LEGACY,"capabilities":{},"clientInfo":{"name":"legacy","version":"1"}}),
        )),
    )
    .await;
    assert_eq!(initialized.status, 200, "{}", initialized.body());
    assert!(initialized.headers.get("mcp-session-id").is_none());
    assert_eq!(initialized.events()[0]["result"]["protocolVersion"], LEGACY);
    let listed = send(
        &server,
        http::Method::POST,
        &legacy(Some(LEGACY)),
        Some(request(2, "tools/list", json!({}))),
    )
    .await;
    assert_eq!(listed.status, 200, "{}", listed.body());
    assert_eq!(
        listed.events()[0]["result"]["tools"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
