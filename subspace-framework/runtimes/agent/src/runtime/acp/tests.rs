//! Pure ACP protocol, transcript and launch rules.
use super::process::resolve;
use super::protocol::{
    Message, capabilities, classify, commands, model_options, permission_option,
    permission_outcome, permission_summary, stop_problem, usage,
};
use super::sessions::validate_session_id;
use super::transcript::{AcpTranscript, Output, fold};
use crate::{Entry, EntryKind, ToolStatus, TranscriptEntry};
use serde_json::{Value, json};
use std::path::PathBuf;
use subspace_core::config::{AcpAgent, AcpConfig};

fn rows(outputs: &[Output]) -> Vec<TranscriptEntry> {
    let mut rows = Vec::new();
    fold(&mut rows, outputs);
    rows
}

fn chunk(kind: &str, text: &str) -> Value {
    json!({"sessionUpdate": kind, "content": {"type": "text", "text": text}})
}

#[test]
fn messages_are_classified_by_their_json_rpc_shape() {
    let response = json!({"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}});
    let Message::Response {
        id: 3,
        result: Ok(result),
    } = classify(&response)
    else {
        panic!("a response with a result")
    };
    assert_eq!(result["stopReason"], "end_turn");
    let failure =
        json!({"jsonrpc":"2.0","id":4,"error":{"code":-32000,"message":"Authentication required"}});
    let Message::Response {
        result: Err(error), ..
    } = classify(&failure)
    else {
        panic!("a response with an error")
    };
    assert!(error.needs_sign_in());
    assert_eq!(error.message, "Authentication required");
    let request =
        json!({"jsonrpc":"2.0","id":"p-1","method":"session/request_permission","params":{}});
    assert!(matches!(
        classify(&request),
        Message::Request {
            method: "session/request_permission",
            ..
        }
    ));
    let update = json!({"jsonrpc":"2.0","method":"session/update","params":{}});
    assert!(matches!(
        classify(&update),
        Message::Notification {
            method: "session/update",
            ..
        }
    ));
    assert_eq!(classify(&json!({"id":"text"})), Message::Invalid);
}

#[test]
fn initialize_answers_report_protocol_and_capabilities() {
    let result = json!({
        "protocolVersion": 1,
        "agentCapabilities": {"loadSession": true, "promptCapabilities": {"image": true}},
        "authMethods": [{"id": "login", "name": "Log in", "description": "Run `agent login`"}]
    });
    let found = capabilities(&result).unwrap();
    assert!(found.load_session);
    assert!(found.images);
    assert_eq!(found.sign_in.as_deref(), Some("Run `agent login`"));
    assert_eq!(found.auth_method.as_deref(), Some("login"));
    assert!(!found.system_prompt);
    let prompting = json!({
        "protocolVersion": 1,
        "agentCapabilities": {"sessionCapabilities": {"systemPrompt": {}}}
    });
    assert!(capabilities(&prompting).unwrap().system_prompt);
    let error = capabilities(&json!({"protocolVersion": 2})).unwrap_err();
    assert!(error.contains("version 2"), "{error}");
}

#[test]
fn approval_answers_pick_one_time_options_first() {
    let options = json!([
        {"optionId": "always", "kind": "allow_always"},
        {"optionId": "once", "kind": "allow_once"},
        {"optionId": "never", "kind": "reject_always"}
    ]);
    assert_eq!(permission_option(&options, true).as_deref(), Some("once"));
    assert_eq!(permission_option(&options, false).as_deref(), Some("never"));
    assert_eq!(permission_option(&json!([]), true), None);
    assert_eq!(
        permission_outcome(Some("once")),
        json!({"outcome": {"outcome": "selected", "optionId": "once"}})
    );
    assert_eq!(
        permission_outcome(None),
        json!({"outcome": {"outcome": "cancelled"}})
    );
    let params = json!({"toolCall": {"title": "Run tests", "rawInput": {"command": "make test"}}});
    assert_eq!(
        permission_summary("OpenCode", &params),
        "OpenCode asks to Run tests (command=make test)"
    );
}

#[test]
fn model_choices_come_from_select_config_options_and_their_groups() {
    let config = json!([
        {"id": "mode", "category": "mode", "type": "select", "currentValue": "ask", "options": []},
        {"id": "model", "category": "model", "type": "select", "currentValue": "b", "options": [
            {"group": "fast", "name": "Fast", "options": [{"value": "a", "name": "Model A"}]},
            {"group": "deep", "name": "Deep", "options": [{"value": "b", "name": "Model B"}]}
        ]},
        {"id": "effort", "category": "thought_level", "type": "select", "currentValue": "low",
         "options": [{"value": "low", "name": "Low"}, {"value": "high", "name": "High"}]}
    ]);
    let models = model_options(&config);
    assert_eq!(models.model_option.as_deref(), Some("model"));
    assert_eq!(models.current.as_deref(), Some("b"));
    let ids: Vec<_> = models
        .choices
        .iter()
        .map(|choice| choice.id.as_str())
        .collect();
    assert_eq!(ids, ["a", "b"]);
    assert_eq!(models.choices[0].label, "Model A");
    assert_eq!(models.choices[0].efforts, ["low", "high"]);
    assert_eq!(models.effort_option.as_deref(), Some("effort"));
    assert!(model_options(&Value::Null).choices.is_empty());
}

#[test]
fn commands_usage_and_stop_reasons_are_read() {
    let update =
        json!({"availableCommands": [{"name": "review", "description": "Review changes"}]});
    let found = commands(&update);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "review");
    let result =
        json!({"stopReason": "max_tokens", "usage": {"inputTokens": 4, "outputTokens": 9}});
    assert_eq!(usage(&result).input_tokens, Some(4));
    assert_eq!(usage(&result).output_tokens, Some(9));
    assert!(stop_problem(&result).is_some());
    assert!(stop_problem(&json!({"stopReason": "end_turn"})).is_none());
    assert!(stop_problem(&json!({"stopReason": "cancelled"})).is_none());
}

#[test]
fn streamed_text_is_confirmed_before_the_tool_call_that_follows_it() {
    let mut transcript = AcpTranscript::default();
    let mut outputs = transcript.user("Find the risks");
    outputs.extend(transcript.apply(&chunk("agent_thought_chunk", "Looking. "), false));
    outputs.extend(transcript.apply(&chunk("agent_message_chunk", "Checking "), false));
    outputs.extend(transcript.apply(&chunk("agent_message_chunk", "now."), false));
    assert!(outputs.contains(&Output::Delta(EntryKind::Assistant, "now.".into())));
    let call = json!({"sessionUpdate": "tool_call", "toolCallId": "c1", "title": "github_read",
        "status": "pending", "rawInput": {"action": "scope"}});
    outputs.extend(transcript.apply(&call, false));
    let done = json!({"sessionUpdate": "tool_call_update", "toolCallId": "c1", "status": "completed",
        "content": [{"type": "content", "content": {"type": "text", "text": "2 repos"}},
                    {"type": "diff", "path": "/tmp/a.txt", "newText": "x"}]});
    outputs.extend(transcript.apply(&done, false));
    outputs.extend(transcript.finish(false));
    let rows = rows(&outputs);
    let kinds: Vec<_> = rows.iter().map(TranscriptEntry::kind).collect();
    assert_eq!(
        kinds,
        [
            EntryKind::User,
            EntryKind::Thinking,
            EntryKind::Assistant,
            EntryKind::Tool
        ]
    );
    assert_eq!(rows[2].entry, Entry::Assistant("Checking now.".into()));
    let call = rows[3].entry.tool().unwrap();
    assert_eq!(call.title, "github_read");
    assert_eq!(call.detail, "scope");
    assert_eq!(call.status, ToolStatus::Done);
    assert_eq!(call.result, "2 repos\nEdited /tmp/a.txt");
}

#[test]
fn a_new_message_id_starts_a_new_row_and_replays_keep_user_messages() {
    let mut transcript = AcpTranscript::default();
    let mut first = chunk("agent_message_chunk", "One");
    first["messageId"] = json!("m1");
    let mut second = chunk("agent_message_chunk", "Two");
    second["messageId"] = json!("m2");
    let mut outputs = transcript.apply(&chunk("user_message_chunk", "Earlier"), true);
    outputs.extend(transcript.apply(&first, true));
    outputs.extend(transcript.apply(&second, true));
    outputs.extend(transcript.finish(false));
    let rows = rows(&outputs);
    assert_eq!(rows.len(), 3);
    assert!(matches!(&rows[0].entry, Entry::User { text, .. } if text == "Earlier"));
    assert!(
        transcript
            .apply(&chunk("user_message_chunk", "echo"), false)
            .is_empty(),
        "a live turn shows Subspace's own copy of the message"
    );
}

#[test]
fn a_replayed_first_prompt_hides_subspace_context() {
    let block = super::context::prompt_block("Boards");
    let block = block["text"].as_str().unwrap();
    let mut transcript = AcpTranscript::default();
    let mut outputs = transcript.apply(&chunk("user_message_chunk", block), true);
    assert!(outputs.is_empty(), "a block on its own adds no row");
    outputs.extend(transcript.apply(&chunk("user_message_chunk", "Hello"), true));
    let joined = format!("{block}\n\nAgain");
    outputs.extend(transcript.apply(&chunk("agent_message_chunk", "Hi"), true));
    outputs.extend(transcript.apply(&chunk("user_message_chunk", &joined), true));
    outputs.extend(transcript.finish(false));
    let rows = rows(&outputs);
    assert!(matches!(&rows[0].entry, Entry::User { text, .. } if text == "Hello"));
    assert!(matches!(&rows[2].entry, Entry::User { text, .. } if text == "Again"));
}

#[test]
fn plans_update_one_row_and_stopped_turns_abort_unfinished_calls() {
    let mut transcript = AcpTranscript::default();
    let plan = |status: &str| json!({"sessionUpdate": "plan", "entries": [{"content": "Read issues", "status": status}]});
    let mut outputs = transcript.apply(&plan("pending"), false);
    outputs.extend(transcript.apply(&plan("completed"), false));
    let call = json!({"sessionUpdate": "tool_call", "toolCallId": "c2", "title": "Run", "status": "in_progress"});
    outputs.extend(transcript.apply(&call, false));
    outputs.extend(transcript.finish(true));
    let rows = rows(&outputs);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].entry, Entry::Info("Plan\n✓ Read issues".into()));
    assert_eq!(rows[1].entry.tool().unwrap().status, ToolStatus::Aborted);
}

fn found(names: &'static [&'static str]) -> impl Fn(&str) -> Option<PathBuf> {
    move |name| {
        names
            .contains(&name)
            .then(|| PathBuf::from(format!("/bin/{name}")))
    }
}

#[test]
fn presets_resolve_an_installed_adapter_before_npx() {
    let opencode = resolve(&AcpConfig::default(), found(&["opencode"])).unwrap();
    assert_eq!(opencode.executable, PathBuf::from("/bin/opencode"));
    assert_eq!(opencode.args, ["acp"]);
    let claude = AcpConfig {
        agent: AcpAgent::Claude,
        ..AcpConfig::default()
    };
    let through_npx = resolve(&claude, found(&["npx"])).unwrap();
    assert_eq!(
        through_npx.args,
        ["-y", "@agentclientprotocol/claude-agent-acp"]
    );
    let installed = resolve(&claude, found(&["npx", "claude-agent-acp"])).unwrap();
    assert_eq!(installed.executable, PathBuf::from("/bin/claude-agent-acp"));
    assert!(installed.args.is_empty());
    let missing = resolve(&claude, found(&[])).unwrap_err();
    assert!(missing.contains("Claude Code was not found"), "{missing}");
}

#[test]
fn a_command_override_keeps_the_preset_arguments_unless_args_are_set() {
    let config = AcpConfig {
        command: Some("my-opencode".into()),
        ..AcpConfig::default()
    };
    let launch = resolve(&config, found(&["my-opencode"])).unwrap();
    assert_eq!(launch.args, ["acp"]);
    let custom = AcpConfig {
        agent: AcpAgent::Custom,
        command: Some("my-agent".into()),
        args: Some(vec!["--stdio".into()]),
        ..AcpConfig::default()
    };
    assert_eq!(
        resolve(&custom, found(&["my-agent"])).unwrap().args,
        ["--stdio"]
    );
    let unset = AcpConfig {
        agent: AcpAgent::Custom,
        ..AcpConfig::default()
    };
    assert!(
        resolve(&unset, found(&["opencode"]))
            .unwrap_err()
            .contains("chat.acp.command")
    );
}

#[test]
fn session_ids_are_printable_and_path_free() {
    assert!(validate_session_id("ses_e2892aadaffemBkOwFQof0sMu2").is_ok());
    assert!(validate_session_id("0b7c-41:2.x").is_ok());
    for invalid in ["", "../etc", "a b", "a/b"] {
        assert!(validate_session_id(invalid).is_err(), "{invalid}");
    }
}
