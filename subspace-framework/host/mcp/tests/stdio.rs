//! MCP over a stdio pipe: raw JSON-RPC in both lifecycles, and the
//! official SDK client.

mod common;

use common::{LEGACY, MODERN, meta, request, server, two_notes};
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, ProtocolVersion};
use rmcp::service::{ClientLifecycleMode, ClientServiceExt};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// A client end of a stdio pipe to a running server.
struct Pipe {
    lines: tokio::io::Lines<BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>>,
    writer: tokio::io::WriteHalf<tokio::io::DuplexStream>,
}

impl Pipe {
    fn open() -> Self {
        Self::with_server(server())
    }

    fn with_server(server: subspace_mcp::ToolServer) -> Self {
        let (client, server_end) = tokio::io::duplex(1 << 16);
        tokio::spawn(async move {
            let running = server.serve(tokio::io::split(server_end)).await?;
            running
                .waiting()
                .await
                .map(|_| ())
                .map_err(|error| error.to_string())?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        });
        let (reader, writer) = tokio::io::split(client);
        Self {
            lines: BufReader::new(reader).lines(),
            writer,
        }
    }

    async fn send(&mut self, message: Value) {
        let line = format!("{message}\n");
        self.writer.write_all(line.as_bytes()).await.unwrap();
    }

    /// Every message up to and including the response to `id`.
    async fn until_response(&mut self, id: u32) -> Vec<Value> {
        let mut messages = Vec::new();
        loop {
            let line = tokio::time::timeout(Duration::from_secs(10), self.lines.next_line())
                .await
                .expect("a response in time")
                .unwrap()
                .expect("an open pipe");
            let message: Value = serde_json::from_str(&line).unwrap();
            let done = message["id"] == id;
            messages.push(message);
            if done {
                return messages;
            }
        }
    }

    async fn call(&mut self, message: Value) -> Value {
        let id = message["id"].as_u64().unwrap() as u32;
        self.send(message).await;
        self.until_response(id).await.pop().unwrap()
    }
}

#[path = "stdio/groups.rs"]
mod groups;

#[tokio::test]
async fn a_modern_stdio_client_discovers_lists_and_calls_without_initialize() {
    let mut pipe = Pipe::open();
    let discovered = pipe
        .call(request(
            1,
            "server/discover",
            json!({"_meta": meta(json!({}))}),
        ))
        .await;
    let result = &discovered["result"];
    let versions = result["supportedVersions"].as_array().unwrap();
    assert!(versions.contains(&json!(MODERN)) && versions.contains(&json!(LEGACY)));
    assert!(result["capabilities"]["tools"].is_object());
    assert!(result["capabilities"]["resources"].is_object());
    assert_eq!(
        result["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "subspace"
    );

    let listed = pipe
        .call(request(2, "tools/list", json!({"_meta": meta(json!({}))})))
        .await;
    let tools = listed["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0]["name"], "echo_read");
    assert_eq!(tools[0]["annotations"]["readOnlyHint"], true);
    assert_eq!(tools[1]["annotations"]["readOnlyHint"], false);
    assert_eq!(tools[1]["annotations"]["destructiveHint"], true);
    assert_eq!(tools[1]["title"], "Notes");

    let echoed = pipe
        .call(request(
            3,
            "tools/call",
            json!({"_meta": meta(json!({})), "name":"echo_read","arguments":{"action":"echo","text":"hi"}}),
        ))
        .await;
    assert_eq!(echoed["result"]["structuredContent"]["text"], "hi");
    assert_eq!(echoed["result"]["isError"], false);
}

#[tokio::test]
async fn stdio_lists_concrete_resources_separately_from_templates() {
    let mut pipe = Pipe::open();
    let resources = pipe
        .call(request(
            1,
            "resources/list",
            json!({"_meta": meta(json!({}))}),
        ))
        .await;
    assert!(
        resources["result"]["resources"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let templates = pipe
        .call(request(
            2,
            "resources/templates/list",
            json!({"_meta": meta(json!({}))}),
        ))
        .await;
    assert!(
        templates["result"]["resourceTemplates"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn progress_reaches_a_stdio_client_before_the_write_result() {
    let mut pipe = Pipe::open();
    let mut params = two_notes();
    params["_meta"] = meta(json!({"progressToken":"notes-1"}));
    pipe.send(request(4, "tools/call", params)).await;
    let messages = pipe.until_response(4).await;
    let progress: Vec<&Value> = messages
        .iter()
        .filter(|message| message["method"] == "notifications/progress")
        .collect();
    assert_eq!(progress.len(), 2, "{messages:?}");
    assert_eq!(progress[0]["params"]["progressToken"], "notes-1");
    assert_eq!(progress[0]["params"]["total"], 2.0);
    assert_eq!(progress[1]["params"]["progress"], 1.0);
    let response = messages.last().unwrap();
    assert_eq!(response["result"]["structuredContent"]["approved"], true);
    assert_eq!(
        response["result"]["structuredContent"]["results"][1]["value"]["body"],
        "second"
    );
}

#[tokio::test]
async fn an_unattended_write_with_a_credential_preview_is_refused() {
    let mut pipe = Pipe::open();
    // Built at runtime so secret scanners do not flag a fixture token.
    let token_shaped = format!("ghp_{}", "1".repeat(36));
    let response = pipe
        .call(request(
            40,
            "tools/call",
            json!({
                "_meta": meta(json!({})),
                "name": "notes_write",
                "arguments": {"operations": [{"action":"note","body":token_shaped}]}
            }),
        ))
        .await;
    assert_eq!(response["result"]["isError"], true);
    assert!(
        response["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("credential")
    );
}

#[tokio::test]
async fn stdio_errors_name_versions_tools_and_bad_arguments() {
    let mut pipe = Pipe::open();
    let mut old = meta(json!({}));
    old["io.modelcontextprotocol/protocolVersion"] = json!("1900-01-01");
    let unsupported = pipe
        .call(request(5, "tools/list", json!({"_meta": old})))
        .await;
    assert_eq!(unsupported["error"]["code"], -32022);
    assert!(
        unsupported["error"]["data"]["supported"]
            .as_array()
            .unwrap()
            .contains(&json!(MODERN))
    );
    let unknown = pipe
        .call(request(
            6,
            "tools/call",
            json!({"_meta": meta(json!({})), "name":"missing"}),
        ))
        .await;
    assert_eq!(unknown["error"]["code"], -32602);
    let invalid = pipe
        .call(request(
            7,
            "tools/call",
            json!({"_meta": meta(json!({})), "name":"echo_read","arguments":{"action":"raw_http"}}),
        ))
        .await;
    assert_eq!(invalid["result"]["isError"], true);
}

#[tokio::test]
async fn a_legacy_stdio_client_initializes_and_calls() {
    let mut pipe = Pipe::open();
    let initialized = pipe
        .call(request(
            1,
            "initialize",
            json!({"protocolVersion":LEGACY,"capabilities":{},"clientInfo":{"name":"legacy","version":"1"}}),
        ))
        .await;
    assert_eq!(initialized["result"]["protocolVersion"], LEGACY);
    pipe.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
        .await;
    let listed = pipe.call(request(2, "tools/list", json!({}))).await;
    assert_eq!(listed["result"]["tools"].as_array().unwrap().len(), 2);
    let echoed = pipe
        .call(request(
            3,
            "tools/call",
            json!({"name":"echo_read","arguments":{"action":"echo","text":"old"}}),
        ))
        .await;
    assert_eq!(echoed["result"]["structuredContent"]["text"], "old");
}

#[tokio::test]
async fn the_official_client_works_in_both_lifecycles() {
    for lifecycle in [
        ClientLifecycleMode::Discover {
            preferred_versions: vec![ProtocolVersion::V_2026_07_28],
        },
        ClientLifecycleMode::Initialize,
    ] {
        let (client_end, server_end) = tokio::io::duplex(1 << 16);
        tokio::spawn(async move {
            if let Ok(running) = server().serve(tokio::io::split(server_end)).await {
                let _ = running.waiting().await;
            }
        });
        let client =
            ().serve_with_lifecycle(tokio::io::split(client_end), lifecycle)
                .await
                .unwrap();
        let tools = client.list_all_tools().await.unwrap();
        assert_eq!(tools.len(), 2);
        let mut arguments = serde_json::Map::new();
        arguments.insert("action".into(), json!("echo"));
        arguments.insert("text".into(), json!("sdk"));
        let result = client
            .call_tool(CallToolRequestParams::new("echo_read").with_arguments(arguments))
            .await
            .unwrap();
        assert_eq!(result.structured_content.unwrap()["text"], "sdk");
        client.cancel().await.unwrap();
    }
}
