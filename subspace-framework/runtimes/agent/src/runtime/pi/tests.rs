// Pi RPC and bridge regressions.
use super::{
    Incoming, Library, PiActor, PiFiles, bridge_registration, decisions, is_tool_request,
    message_entries, messages, model_choices, tool_request,
};
use crate::{ChatEvent, Entry, Event, ToolStatus};
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use subspace_agent_tools::{Access, AgentTool, Catalog, Invocation, ToolSpec};
use tokio::sync::Notify;
use tokio::sync::mpsc;

struct BatchTool {
    spec: ToolSpec,
    mutations: Arc<AtomicUsize>,
    first: Arc<Notify>,
    release: Arc<Notify>,
}

#[async_trait]
impl AgentTool for BatchTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    async fn call(&self, invocation: Invocation) -> Result<serde_json::Value, String> {
        if !invocation
            .approval
            .approve("Apply two fixture mutations".into())
            .await?
        {
            return Err("Denied".into());
        }
        self.mutations.fetch_add(1, Ordering::SeqCst);
        self.first.notify_one();
        self.release.notified().await;
        if invocation.monitor.cancelled() {
            return Err("Cancelled".into());
        }
        self.mutations.fetch_add(1, Ordering::SeqCst);
        Ok(json!({"mutations": 2}))
    }
}

fn fixture_batch(
    name: &'static str,
) -> (
    Arc<dyn AgentTool>,
    Arc<AtomicUsize>,
    Arc<Notify>,
    Arc<Notify>,
) {
    let mutations = Arc::new(AtomicUsize::new(0));
    let first = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let tool: Arc<dyn AgentTool> = Arc::new(BatchTool {
        spec: ToolSpec {
            name,
            title: name,
            description: "Harmless fixture tool.",
            access: Access::Write,
            arguments: subspace_agent_tools::Arguments::Single { fields: Vec::new() },
            approval: subspace_agent_tools::ApprovalPolicy::PerCall,
        },
        mutations: mutations.clone(),
        first: first.clone(),
        release: release.clone(),
    });
    (tool, mutations, first, release)
}

fn test_actor(dir: &std::path::Path, tools: Catalog) -> PiActor {
    let events = Arc::new(super::EventQueue::default());
    let decisions = decisions(events.clone(), dir);
    let (sender, incoming) = mpsc::channel(super::EVENT_LIMIT);
    PiActor {
        tools,
        files: PiFiles {
            workspace: dir.into(),
            executable: dir.join("pi"),
            bridge: dir.join("bridge.ts"),
            sessions_dir: dir.join("sessions"),
            tabs_file: dir.join("tabs.json"),
            library_file: dir.join("library.json"),
        },
        decisions,
        events,
        chats: HashMap::new(),
        cancellations: HashMap::new(),
        order: Vec::new(),
        active: 1,
        library: Library::default(),
        incoming,
        sender,
    }
}

async fn approval_request(actor: &PiActor) -> String {
    loop {
        if let Some(Event::Chat {
            event: ChatEvent::Approval { request_id, .. },
            ..
        }) = super::super::lock(&actor.events.items).pop_front()
        {
            return request_id;
        }
        tokio::task::yield_now().await;
    }
}

fn bridge_request() -> serde_json::Value {
    json!({
        "type":"extension_ui_request", "id":"bridge-1", "method":"editor",
        "title":"subspace-tool:fixture_write", "prefill":"{}"
    })
}

#[test]
fn bridge_registers_only_the_supplied_catalog_tool() {
    let registration =
        bridge_registration(&ToolSpec {
            name: "github_read",
            title: "Read GitHub",
            description: "Read configured repositories.",
            access: Access::Read,
            arguments: subspace_agent_tools::schema::single(vec![
                subspace_agent_tools::schema::field("repo", json!({"type":"string"}), true),
            ]),
            approval: subspace_agent_tools::ApprovalPolicy::None,
        })
        .unwrap();
    assert!(registration.contains("pi.registerTool"));
    assert!(registration.contains("github_read"));
    assert!(registration.contains("ctx.ui.editor"));
    assert!(!registration.contains("registerMcpServer"));
}

#[test]
fn extension_request_names_and_decodes_one_catalog_call() {
    let request = json!({
        "type":"extension_ui_request", "id":"one", "method":"editor",
        "title":"subspace-tool:ado_write", "prefill":"{\"operations\":[1]}"
    });
    assert!(is_tool_request(&request));
    assert_eq!(
        tool_request(&request),
        Some(("ado_write".into(), json!({"operations":[1]})))
    );
    assert!(!is_tool_request(
        &json!({"type":"extension_ui_request","method":"confirm"})
    ));
}

#[tokio::test]
async fn denied_bridge_catalog_approval_performs_no_mutations() {
    let dir = tempfile::tempdir().unwrap();
    let (tool, mutations, _first, release) = fixture_batch("fixture_write");
    let mut actor = test_actor(dir.path(), Catalog::new(vec![tool]).unwrap());
    actor
        .cancellations
        .insert(4, Arc::new(std::sync::atomic::AtomicBool::new(false)));
    actor.call_tool(4, bridge_request());
    let request = approval_request(&actor).await;
    actor.decisions.approve(&request, false);
    release.notify_one();
    let (_, Incoming::ToolResult { result, .. }) = actor.incoming.recv().await.unwrap() else {
        panic!("expected the denied bridge result");
    };
    assert!(result.is_err());
    assert_eq!(mutations.load(Ordering::SeqCst), 0);
}

#[path = "boundary_tests.rs"]
mod boundary_tests;

#[tokio::test]
async fn exited_pi_cancels_an_approved_catalog_batch_before_its_next_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let (tool, mutations, first, release) = fixture_batch("fixture_write");
    let mut actor = test_actor(dir.path(), Catalog::new(vec![tool]).unwrap());
    actor
        .cancellations
        .insert(5, Arc::new(std::sync::atomic::AtomicBool::new(false)));
    actor.call_tool(5, bridge_request());
    let request = approval_request(&actor).await;
    actor.decisions.approve(&request, true);
    first.notified().await;
    actor.process_exit(5, "fixture exit").await;
    release.notify_one();
    let (_, Incoming::ToolResult { result, .. }) = actor.incoming.recv().await.unwrap() else {
        panic!("expected the cancelled bridge result");
    };
    assert!(result.is_err());
    assert_eq!(mutations.load(Ordering::SeqCst), 1);
}

#[test]
fn configured_models_do_not_invent_an_unknown_or_default_provider() {
    assert!(model_choices(&json!([])).is_empty());
    let models = model_choices(
        &json!([{"provider":"anthropic","id":"sonnet","name":"Sonnet","reasoning":true}]),
    );
    assert_eq!(models[0].id, "anthropic/sonnet");
    assert!(models[0].efforts.contains(&"high".to_owned()));
}

#[test]
fn offline_transcript_maps_stream_finals_thinking_and_tools() {
    let history = messages(&json!([
        {"role":"user","content":"Check it","timestamp":1},
        {"role":"assistant","timestamp":2,"content":[
            {"type":"thinking","thinking":"Inspect"},
            {"type":"toolCall","id":"c1","name":"github_read","arguments":{"repo":"o/r"}},
            {"type":"text","text":"Done"}
        ]},
        {"role":"toolResult","toolCallId":"c1","toolName":"github_read","content":[{"type":"text","text":"ok"}],"isError":false,"timestamp":3}
    ]));
    assert!(matches!(history[0].entry, Entry::User { .. }));
    assert!(
        history
            .iter()
            .any(|entry| matches!(entry.entry, Entry::Thinking { .. }))
    );
    assert!(
        history
            .iter()
            .any(|entry| matches!(entry.entry, Entry::Assistant(ref text) if text == "Done"))
    );
    assert!(history.iter().any(|entry| {
        entry
            .entry
            .tool()
            .is_some_and(|call| call.status == ToolStatus::Done)
    }));
    assert_eq!(
        message_entries(
            &json!({"role":"user","content":"steer","timestamp":4}),
            true
        )[0]
        .entry,
        Entry::User {
            text: "steer".into(),
            steering: true
        }
    );
}
