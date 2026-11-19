//! Pure Pi RPC and transcript mapping.
use super::{
    ChatId, Command, Entry, EntryKind, EventQueue, HashMap, MAX_CHATS, ModelChoice, Path, PathBuf,
    SESSION_ID_LIMIT, THINKING_LEVELS, ToolStatus, TranscriptEntry, Usage, Value, fs,
};
use crate::ToolCall;
use crate::runtime::transcript;
pub(in crate::runtime) fn command_chat(command: &Command) -> Option<ChatId> {
    match command {
        Command::Chat { chat, .. }
        | Command::OpenChat { chat, .. }
        | Command::OpenRemote { chat, .. } => Some(*chat),
        _ => None,
    }
}

/// A failed command's error, shown in its chat when it has one.
pub(in crate::runtime) fn command_error(chat: Option<ChatId>, error: String) -> crate::Event {
    match chat {
        Some(chat) => crate::Event::of(chat, crate::ChatEvent::Error(error)),
        None => crate::Event::Error(error),
    }
}

pub(super) fn room_for_tab(open: usize) -> Result<(), String> {
    (open < MAX_CHATS)
        .then_some(())
        .ok_or_else(|| format!("At most {MAX_CHATS} chats are open at once. Close a tab first."))
}

pub(in crate::runtime) fn restored_active(
    restored: &[(ChatId, String)],
    saved: Option<&str>,
) -> ChatId {
    saved
        .and_then(|active| restored.iter().find(|(_, session)| session == active))
        .or_else(|| restored.first())
        .map_or(1, |(chat, _)| *chat)
}

pub(super) fn validate_session_id(id: &str) -> Result<(), String> {
    let valid = !id.is_empty()
        && id.len() <= SESSION_ID_LIMIT
        && id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-');
    valid
        .then_some(())
        .ok_or_else(|| "That Pi session id is invalid.".into())
}

pub(super) fn is_tool_request(record: &Value) -> bool {
    record["type"].as_str() == Some("extension_ui_request")
        && record["method"].as_str() == Some("editor")
        && record["title"]
            .as_str()
            .is_some_and(|title| title.starts_with("subspace-tool:"))
}

pub(super) fn tool_request(record: &Value) -> Option<(String, Value)> {
    let name = record["title"]
        .as_str()?
        .strip_prefix("subspace-tool:")?
        .to_owned();
    let arguments = serde_json::from_str(record["prefill"].as_str()?).ok()?;
    Some((name, arguments))
}

pub(super) fn model_id(model: &Value) -> Option<String> {
    let provider = model["provider"].as_str()?;
    let id = model["id"].as_str()?;
    Some(format!("{provider}/{id}"))
}

pub(super) fn model_choices(models: &Value) -> Vec<ModelChoice> {
    models
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|model| {
            let id = model_id(model)?;
            let label = model["name"].as_str().unwrap_or(&id).to_owned();
            let efforts = if model["reasoning"].as_bool().unwrap_or(false) {
                THINKING_LEVELS
                    .iter()
                    .map(|level| (*level).to_owned())
                    .collect()
            } else {
                Vec::new()
            };
            Some(ModelChoice {
                id,
                label,
                efforts,
                default_effort: None,
            })
        })
        .collect()
}

pub(super) fn stream_delta(record: &Value, events: &EventQueue, chat: ChatId) {
    let update = &record["assistantMessageEvent"];
    let kind = match update["type"].as_str() {
        Some("text_delta") => EntryKind::Assistant,
        Some("thinking_delta") => EntryKind::Thinking,
        _ => return,
    };
    if let Some(delta) = update["delta"].as_str() {
        events.delta(chat, kind, delta.to_owned());
    }
}

pub(super) fn tool_entry(record: &Value) -> TranscriptEntry {
    let call_id = record["toolCallId"].as_str().unwrap_or_default();
    let done = record["type"].as_str() == Some("tool_execution_end");
    let failed = record["isError"].as_bool().unwrap_or(false);
    TranscriptEntry {
        id: format!("tool:{call_id}"),
        entry: Entry::Tool(ToolCall {
            call_id: call_id.to_owned(),
            title: record["toolName"].as_str().unwrap_or("Tool").to_owned(),
            detail: transcript::argument_summary(&record["args"]),
            status: if !done {
                ToolStatus::Running
            } else if failed {
                ToolStatus::Failed
            } else {
                ToolStatus::Done
            },
            result: if done {
                content_text(&record["result"]["content"])
            } else {
                String::new()
            },
        }),
    }
}

pub(super) fn message_entries(message: &Value, steering: bool) -> Vec<TranscriptEntry> {
    let timestamp = message["timestamp"].as_i64().unwrap_or_default();
    let role = message["role"].as_str().unwrap_or_default();
    if role == "user" {
        return vec![TranscriptEntry {
            id: format!("pi:{timestamp}:user"),
            entry: Entry::User {
                text: message_text(message),
                steering,
            },
        }];
    }
    if role != "assistant" {
        return Vec::new();
    }
    message["content"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(index, block)| {
            let entry = match block["type"].as_str()? {
                "text" => Entry::Assistant(block["text"].as_str()?.to_owned()),
                "thinking" => Entry::Thinking {
                    text: block["thinking"].as_str()?.to_owned(),
                    seconds: None,
                },
                _ => return None,
            };
            Some(TranscriptEntry {
                id: format!("pi:{timestamp}:{index}"),
                entry,
            })
        })
        .collect()
}

pub(super) fn message_text(message: &Value) -> String {
    match &message["content"] {
        Value::String(text) => text.clone(),
        blocks => content_text(blocks),
    }
}

pub(super) fn content_text(content: &Value) -> String {
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|block| block["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn pi_usage(usage: &Value) -> Usage {
    Usage {
        input_tokens: usage["input"].as_u64(),
        output_tokens: usage["output"].as_u64(),
        nano_aiu: None,
    }
}

pub(super) fn messages(value: &Value) -> Vec<TranscriptEntry> {
    let mut entries = Vec::new();
    let mut tools: HashMap<String, (String, Value)> = HashMap::new();
    for message in value.as_array().into_iter().flatten() {
        if message["role"].as_str() == Some("assistant") {
            remember_tool_calls(message, &mut tools);
        }
        if message["role"].as_str() == Some("toolResult") {
            entries.push(history_tool(message, &tools));
        } else {
            entries.extend(message_entries(message, false));
        }
    }
    entries
}

pub(super) fn remember_tool_calls(message: &Value, tools: &mut HashMap<String, (String, Value)>) {
    for block in message["content"].as_array().into_iter().flatten() {
        if block["type"].as_str() == Some("toolCall")
            && let Some(id) = block["id"].as_str()
        {
            tools.insert(
                id.to_owned(),
                (
                    block["name"].as_str().unwrap_or("Tool").to_owned(),
                    block["arguments"].clone(),
                ),
            );
        }
    }
}

pub(super) fn history_tool(
    message: &Value,
    tools: &HashMap<String, (String, Value)>,
) -> TranscriptEntry {
    let id = message["toolCallId"].as_str().unwrap_or_default();
    let (name, arguments) = tools.get(id).cloned().unwrap_or_else(|| {
        (
            message["toolName"].as_str().unwrap_or("Tool").to_owned(),
            Value::Null,
        )
    });
    TranscriptEntry {
        id: format!("tool:{id}"),
        entry: Entry::Tool(ToolCall {
            call_id: id.to_owned(),
            title: name,
            detail: transcript::argument_summary(&arguments),
            status: if message["isError"].as_bool().unwrap_or(false) {
                ToolStatus::Failed
            } else {
                ToolStatus::Done
            },
            result: content_text(&message["content"]),
        }),
    }
}

pub(super) fn read_pi_history(path: &str, sessions: &Path) -> Result<Vec<TranscriptEntry>, String> {
    let text = fs::read_to_string(session_file(path, sessions)?)
        .map_err(|_| "Cannot read that Pi chat's saved transcript.".to_string())?;
    let messages: Vec<Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|entry| entry["type"].as_str() == Some("message"))
        .map(|entry| entry["message"].clone())
        .collect();
    Ok(crate::runtime::pi::messages(&Value::Array(messages)))
}

pub(super) fn remove_session_file(path: &str, sessions: &Path) -> Result<(), String> {
    fs::remove_file(session_file(path, sessions)?)
        .map_err(|_| "Cannot delete that Pi chat.".to_string())
}

pub(super) fn session_file(path: &str, sessions: &Path) -> Result<PathBuf, String> {
    let file = Path::new(path)
        .canonicalize()
        .map_err(|_| "Cannot locate that Pi session file.".to_string())?;
    let root = sessions
        .canonicalize()
        .map_err(|_| "Cannot verify Pi session storage.".to_string())?;
    if !file.starts_with(root) || !file.is_file() {
        return Err("Pi session path escaped Subspace's private storage.".into());
    }
    Ok(file)
}
