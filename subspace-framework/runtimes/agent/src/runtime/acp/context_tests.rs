//! The fake ACP agent receiving the app's tools and Subspace's session
//! context: the context as a `systemPrompt` when it advertises one, else
//! once with the first prompt; the tools as an HTTP MCP server or a relay.
use super::fixture::{
    FAKE_AGENT, FAKE_TOKEN, FakeTools, Fixture, approve, is_chat, ready, rows, say, send,
    start_with_tools, turn_done,
};
use crate::{ChatEvent, Command, Entry, Event};
use std::sync::Arc;

const CONTEXT: &str = "Subspace context fixture: boards github, irc.";

/// The fake agent, writing each request it reads to `requests.log`, and
/// advertising `systemPrompt` when `system_prompt` is set.
fn logging_agent(system_prompt: bool) -> String {
    let script = FAKE_AGENT
        .replace(
            "while IFS= read -r line; do\n",
            "while IFS= read -r line; do\n  printf '%s\\n' \"$line\" >>requests.log\n",
        )
        .replace(
            r#"case "$line" in *'"mcpServers":[]'*) ;; *) echo "unexpected MCP servers" >&2; exit 4 ;; esac"#,
            "",
        );
    if system_prompt {
        script.replace(
            r#"{"loadSession":true}"#,
            r#"{"loadSession":true,"sessionCapabilities":{"systemPrompt":{}}}"#,
        )
    } else {
        script
    }
}

/// The requests the agent read with `method`, in order.
fn requests(fixture: &Fixture, method: &str) -> Vec<serde_json::Value> {
    let log = std::fs::read_to_string(fixture.directory.path().join("requests.log")).unwrap();
    log.lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .filter(|request| request["method"] == method)
        .collect()
}

fn tools() -> Arc<FakeTools> {
    Arc::new(FakeTools {
        instructions: CONTEXT,
        approvals: Default::default(),
    })
}

async fn two_turns(fixture: &mut Fixture) {
    for _ in 0..2 {
        say(fixture, "Write my notes").await;
        approve(fixture, true).await;
        fixture.wait_for(turn_done).await;
    }
}

#[tokio::test]
async fn without_system_prompt_support_only_the_first_prompt_carries_the_context() {
    let mut fixture = start_with_tools(&logging_agent(false), None, Some(tools()));
    fixture.wait_for(ready).await;
    two_turns(&mut fixture).await;
    let opened = requests(&fixture, "session/new");
    assert!(opened[0]["params"].get("systemPrompt").is_none());
    let prompts = requests(&fixture, "session/prompt");
    assert_eq!(prompts.len(), 2);
    let first = prompts[0]["params"]["prompt"].as_array().unwrap();
    assert_eq!(first.len(), 2);
    let lead = first[0]["text"].as_str().unwrap();
    assert!(lead.starts_with("<subspace-context>") && lead.contains(CONTEXT));
    assert_eq!(first[1]["text"], "Write my notes");
    let second = prompts[1]["params"]["prompt"].as_array().unwrap();
    assert_eq!(second.len(), 1);
    assert!(!prompts[1].to_string().contains(CONTEXT));
    let rows = rows(&fixture.seen);
    assert!(
        rows.iter()
            .all(|row| !matches!(row, Entry::User { text, .. } if text.contains(CONTEXT)))
    );
}

#[tokio::test]
async fn with_system_prompt_support_session_requests_carry_the_context_and_prompts_do_not() {
    let mut fixture = start_with_tools(&logging_agent(true), None, Some(tools()));
    fixture.wait_for(ready).await;
    two_turns(&mut fixture).await;
    let opened = requests(&fixture, "session/new");
    assert_eq!(
        opened[0]["params"]["systemPrompt"],
        serde_json::json!({"mode": "append", "content": [{"type": "text", "text": CONTEXT}]})
    );
    let prompts = requests(&fixture, "session/prompt");
    assert!(
        prompts
            .iter()
            .all(|prompt| !prompt.to_string().contains(CONTEXT))
    );

    let mut reopened = start_with_tools(
        &logging_agent(true),
        Some(r#"{"sessions":["fake-1"],"active":"fake-1"}"#),
        Some(tools()),
    );
    reopened.wait_for(ready).await;
    let loaded = requests(&reopened, "session/load");
    assert_eq!(
        loaded[0]["params"]["systemPrompt"]["content"][0]["text"],
        CONTEXT
    );
}

#[tokio::test]
async fn a_reopened_session_without_system_prompt_support_gets_no_context_again() {
    let mut fixture = start_with_tools(
        &logging_agent(false),
        Some(r#"{"sessions":["fake-1"],"active":"fake-1"}"#),
        Some(tools()),
    );
    fixture.wait_for(ready).await;
    say(&fixture, "Write my notes").await;
    approve(&mut fixture, true).await;
    fixture.wait_for(turn_done).await;
    assert!(
        requests(&fixture, "session/load")[0]["params"]
            .get("systemPrompt")
            .is_none()
    );
    let prompts = requests(&fixture, "session/prompt");
    assert!(!prompts[0].to_string().contains(CONTEXT));
}

#[tokio::test]
async fn each_session_gets_the_app_endpoint_and_its_writes_ask_the_chat() {
    let fake = tools();
    let http_agent = logging_agent(false).replace(
        r#"{"loadSession":true}"#,
        r#"{"loadSession":true,"mcpCapabilities":{"http":true}}"#,
    );
    let mut fixture = start_with_tools(&http_agent, None, Some(fake.clone()));
    fixture.wait_for(ready).await;
    let servers = &requests(&fixture, "session/new")[0]["params"]["mcpServers"];
    assert_eq!(servers[0]["type"], "http");
    assert_eq!(servers[0]["name"], "subspace");
    assert_eq!(servers[0]["url"], "http://127.0.0.1:9/mcp");
    assert_eq!(
        servers[0]["headers"][0]["value"],
        format!("Bearer {FAKE_TOKEN}")
    );

    let approval = fake.approvals.lock().unwrap()[0].clone();
    let asked = tokio::spawn(async move { approval.approve(format!("Send {FAKE_TOKEN}")).await });
    let refused = asked.await.unwrap();
    assert!(refused.is_err(), "a summary holding the token is refused");
    let approval = fake.approvals.lock().unwrap()[0].clone();
    let asked = tokio::spawn(async move { approval.approve("Send hello to #ops".into()).await });
    let card = fixture
        .wait_for(|event| is_chat(event, |event| matches!(event, ChatEvent::Approval { .. })))
        .await;
    let Event::Chat {
        event: ChatEvent::Approval {
            request_id,
            summary,
        },
        ..
    } = card
    else {
        unreachable!()
    };
    assert_eq!(summary, "Send hello to #ops");
    send(
        &fixture,
        Command::Approve {
            request_id,
            allow: true,
        },
    )
    .await;
    assert!(asked.await.unwrap().unwrap());
    let seen = format!("{:?}", fixture.seen);
    assert!(
        !seen.contains(FAKE_TOKEN),
        "the token never reaches an event"
    );
    let logged = subspace_core::log::recent()
        .iter()
        .any(|record| record.line().contains(FAKE_TOKEN));
    assert!(!logged, "the token is never logged");
}

#[tokio::test]
async fn an_agent_without_http_mcp_gets_the_stdio_relay() {
    let mut fixture = start_with_tools(&logging_agent(false), None, Some(tools()));
    fixture.wait_for(ready).await;
    let servers = &requests(&fixture, "session/new")[0]["params"]["mcpServers"];
    assert!(servers[0].get("type").is_none());
    assert_eq!(
        servers[0]["args"],
        serde_json::json!(["mcp", "--attach", "http://127.0.0.1:9/mcp"])
    );
    assert_eq!(servers[0]["env"][0]["value"], FAKE_TOKEN);
}
