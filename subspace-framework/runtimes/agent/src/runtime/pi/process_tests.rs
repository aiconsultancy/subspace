//! Pi process-boundary regressions.
#[cfg(unix)]
use super::fixture_tests::write_executable;
#[cfg(unix)]
use super::{Incoming, PiChat, PiMessage, verify_pi_with_timeout};
use super::{SpawnConfig, pi_command, pi_process, write_bridge};
#[cfg(unix)]
use crate::ChatEvent;
use async_trait::async_trait;
use serde_json::json;
use std::fs;
use std::sync::Arc;
use subspace_agent_tools::{Access, AgentTool, Catalog, Invocation, ToolSpec};
#[cfg(unix)]
use tokio::sync::mpsc;

// Process startup can exceed one second on busy macOS builders. These test-only
// budgets retain bounded waits without changing production RPC or probe limits.
#[cfg(unix)]
pub(super) const FIXTURE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

struct FixtureTool(ToolSpec);

#[async_trait]
impl AgentTool for FixtureTool {
    fn spec(&self) -> &ToolSpec {
        &self.0
    }

    async fn call(&self, _invocation: Invocation) -> Result<serde_json::Value, String> {
        Ok(json!({}))
    }
}

fn fixture_tool(name: &'static str) -> Arc<dyn AgentTool> {
    Arc::new(FixtureTool(ToolSpec {
        name,
        title: name,
        description: "Harmless fixture tool.",
        access: Access::Read,
        arguments: subspace_agent_tools::Arguments::Single { fields: Vec::new() },
        approval: subspace_agent_tools::ApprovalPolicy::None,
    }))
}

#[cfg(unix)]
pub(super) async fn spawn_fake_chat(
    script_text: &str,
    id: u64,
) -> (tempfile::TempDir, PiChat, mpsc::Receiver<PiMessage>) {
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("fake-pi");
    write_executable(&script, script_text);
    let bridge = directory.path().join("bridge.ts");
    fs::write(&bridge, "export default function() {}\n").unwrap();
    let sessions = directory.path().join("sessions");
    fs::create_dir(&sessions).unwrap();
    let (sender, receiver) = mpsc::channel(16);
    let chat = PiChat::spawn(
        SpawnConfig {
            executable: &script,
            workspace: directory.path(),
            bridge: &bridge,
            sessions: &sessions,
            saved: None,
            title: None,
        },
        id,
        sender,
    )
    .await
    .unwrap();
    (directory, chat, receiver)
}
#[test]
fn pi_starts_with_native_tools_and_all_automatic_resources_disabled() {
    let config = SpawnConfig {
        executable: std::path::Path::new("/opt/homebrew/bin/pi"),
        workspace: std::path::Path::new("/private/workspace"),
        bridge: std::path::Path::new("/private/workspace/bridge.ts"),
        sessions: std::path::Path::new("/private/workspace/sessions"),
        saved: Some("abc-123"),
        title: None,
    };
    let command = pi_command(&config);
    let args: Vec<_> = command
        .as_std()
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    for required in [
        "--no-extensions",
        "--no-mcp",
        "--no-builtin-tools",
        "--no-context-files",
        "--no-prompt-templates",
        "--no-skills",
        "--no-approve",
    ] {
        assert!(
            args.iter().any(|arg| arg == required),
            "missing {required}: {args:?}"
        );
    }
    assert_eq!(args.iter().filter(|arg| *arg == "--extension").count(), 1);
    assert!(!args.iter().any(|arg| arg.contains("mcp__")));
}

#[test]
fn version_probe_uses_the_same_sanitized_finder_safe_process_environment() {
    let executable = std::path::Path::new("/opt/homebrew/bin/pi");
    let command = pi_process(executable);
    let path = command
        .as_std()
        .get_envs()
        .find_map(|(name, value)| {
            (name == "PATH")
                .then_some(value)?
                .map(|path| path.to_string_lossy().into_owned())
        })
        .unwrap();
    assert!(path.starts_with("/opt/homebrew/bin"), "{path}");
    for secret in ["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"] {
        assert!(
            command
                .as_std()
                .get_envs()
                .any(|(name, value)| name == secret && value.is_none())
        );
    }
}

#[cfg(unix)]
#[path = "environment_tests.rs"]
mod environment_tests;

#[cfg(unix)]
#[tokio::test]
async fn timed_out_version_probe_kills_its_child() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("pi");
    let pid = dir.path().join("pid");
    write_executable(
        &script,
        &format!(
            "#!/bin/sh\necho $$ > '{}'\nwhile :; do sleep 1; done\n",
            pid.display()
        ),
    );
    let probe = tokio::spawn(async move { verify_pi_with_timeout(&script, FIXTURE_TIMEOUT).await });
    let child_pid = tokio::time::timeout(FIXTURE_TIMEOUT, async {
        loop {
            if let Ok(contents) = fs::read_to_string(&pid)
                && let Ok(child_pid) = contents.trim().parse::<u32>()
            {
                break child_pid;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("fixture did not report its PID");
    assert!(
        std::process::Command::new("kill")
            .args(["-0", &child_pid.to_string()])
            .status()
            .unwrap()
            .success(),
        "fixture child must be running before the probe times out"
    );
    let error = probe.await.unwrap().unwrap_err();
    assert!(error.contains("timed out"), "{error}");
    let status = std::process::Command::new("kill")
        .args(["-0", &child_pid.to_string()])
        .status()
        .unwrap();
    assert!(!status.success(), "timed-out Pi version child still runs");
}

#[cfg(unix)]
#[tokio::test]
async fn version_probe_accepts_only_bounded_exact_output() {
    let dir = tempfile::tempdir().unwrap();
    let valid = dir.path().join("valid-pi");
    write_executable(&valid, "#!/bin/sh\nprintf '1.0.4\\n'\n");
    let valid_result = verify_pi_with_timeout(&valid, FIXTURE_TIMEOUT).await;
    assert!(valid_result.is_ok(), "{valid_result:?}");
    let noisy = dir.path().join("noisy-pi");
    write_executable(&noisy, "#!/bin/sh\nyes x | head -c 5000\n");
    assert!(
        verify_pi_with_timeout(&noisy, FIXTURE_TIMEOUT)
            .await
            .is_err()
    );
}

#[cfg(unix)]
pub(super) async fn apply_next_response(
    chat: &mut PiChat,
    receiver: &mut mpsc::Receiver<PiMessage>,
    events: &super::EventQueue,
) {
    let (_, Incoming::Record(record)) = tokio::time::timeout(FIXTURE_TIMEOUT, receiver.recv())
        .await
        .unwrap()
        .unwrap()
    else {
        panic!("expected RPC response")
    };
    chat.receive(&record, events).await.unwrap();
}

#[cfg(unix)]
pub(super) async fn started_confirmation_chat(
    extra_cases: &str,
    id: u64,
) -> (
    tempfile::TempDir,
    PiChat,
    mpsc::Receiver<PiMessage>,
    super::EventQueue,
) {
    let mut script = String::from(
        r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *startup-state*) printf '%s\n' '{"type":"response","id":"startup-state","command":"get_state","success":true,"data":{"sessionId":"fake-1","model":{"provider":"unknown","id":"unknown"},"thinkingLevel":"off"}}' ;;
    *startup-models*) printf '%s\n' '{"type":"response","id":"startup-models","command":"get_available_models","success":true,"data":{"models":[]}}' ;;
    *startup-history*) printf '%s\n' '{"type":"response","id":"startup-history","command":"get_messages","success":true,"data":{"messages":[]}}' ;;
"#,
    );
    script.push_str(extra_cases);
    script.push_str("  esac\ndone\n");
    let (directory, mut chat, mut receiver) = spawn_fake_chat(&script, id).await;
    let events = super::EventQueue::default();
    apply_next_response(&mut chat, &mut receiver, &events).await;
    apply_next_response(&mut chat, &mut receiver, &events).await;
    apply_next_response(&mut chat, &mut receiver, &events).await;
    (directory, chat, receiver, events)
}

#[cfg(unix)]
fn model_state(id: &str, model: &str) -> serde_json::Value {
    let (provider, model_id) = model.split_once('/').unwrap();
    json!({
        "type":"response", "id":id, "command":"get_state", "success":true,
        "data":{"sessionId":"fake-1","model":{"provider":provider,"id":model_id},"thinkingLevel":"off"}
    })
}

#[cfg(unix)]
pub(super) fn confirmation_count(events: &super::EventQueue) -> usize {
    super::super::lock(&events.items)
        .iter()
        .filter(|event| {
            matches!(
                event,
                super::super::Event::Chat {
                    event: ChatEvent::ModelConfirmed { .. },
                    ..
                }
            )
        })
        .count()
}

#[cfg(unix)]
pub(super) async fn confirm_model(chat: &mut PiChat, events: &super::EventQueue) {
    chat.set_model("local/second", None, true).await.unwrap();
    let pending = chat.pending_model.as_ref().unwrap();
    let set_request_id = pending.set_request_id.clone();
    let state_request_id = pending.state_request_id.clone();
    chat.receive(
        &json!({"type":"response","id":set_request_id,"command":"set_model","success":true}),
        events,
    )
    .await
    .unwrap();
    chat.receive(&model_state(&state_request_id, "local/second"), events)
        .await
        .unwrap();
}

#[cfg(unix)]
pub(super) async fn reject_model(chat: &mut PiChat, events: &super::EventQueue) {
    chat.set_model("local/rejected", None, true).await.unwrap();
    let rejected_id = chat.pending_model.as_ref().unwrap().set_request_id.clone();
    chat.receive(
        &json!({"type":"response","id":rejected_id,"command":"set_model","success":false,"error":"rejected"}),
        events,
    )
    .await
    .unwrap_err();
    chat.receive(&model_state("unrelated-state", "local/second"), events)
        .await
        .unwrap();
}

#[test]
fn generated_bridge_registers_only_catalog_tools() {
    let directory = tempfile::tempdir().unwrap();
    let catalog = Catalog::new(vec![
        fixture_tool("fixture_read"),
        fixture_tool("fixture_write"),
    ])
    .unwrap();
    let path = write_bridge(directory.path(), &catalog).unwrap();
    assert_eq!(path, directory.path().join("subspace-pi-bridge.ts"));
    let source = fs::read_to_string(path).unwrap();
    assert!(source.contains("fixture_read"));
    assert!(source.contains("fixture_write"));
}
