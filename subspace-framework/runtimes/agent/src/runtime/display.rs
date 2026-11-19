use super::choices::model_choice;
use super::names::session_summaries;
use super::transcript::{Transcript, UNFINISHED};
use super::{
    ChatEvent, ChatId, Choice, Client, ContextRef, Entry, EntryKind, Event, EventQueue, HashSet,
    PROMPT_LIMIT, Session, ToolStatus, TranscriptEntry, Usage, Value, contains_secret,
};

/// A non-negative token counter the SDK reported, if it reported one.
fn count(data: &Value, field: &str) -> Option<u64> {
    data[field]
        .as_i64()
        .and_then(|value| u64::try_from(value).ok())
}

/// Publishes everything a chat's toolbar shows, and returns the confirmed
/// model selection.
pub(super) async fn refresh(
    client: &Client,
    session: &Session,
    chat: ChatId,
    events: &EventQueue,
) -> Option<ChatEvent> {
    let selected = super::selection::publish_current(session, chat, events).await;
    refresh_models(client, events).await;
    refresh_agents(session, events).await;
    refresh_commands(session, events).await;
    selected
}

pub(super) async fn refresh_models(client: &Client, events: &EventQueue) {
    match client.rpc().models().list().await {
        Ok(models) => {
            events
                .push(Event::Models(
                    models.models.into_iter().map(model_choice).collect(),
                ))
                .await
        }
        Err(error) => events.push(Event::Error(model_list_error(&error))).await,
    }
}

fn model_list_error(error: &github_copilot_sdk::Error) -> String {
    use github_copilot_sdk::ErrorKind;

    let detail = match error.kind() {
        ErrorKind::Rpc { code } => format!("runtime RPC error (code {code})"),
        ErrorKind::Json => "invalid JSON request or response".into(),
        ErrorKind::Io => "transport I/O failure".into(),
        ErrorKind::Protocol(_) => "transport or protocol failure".into(),
        ErrorKind::Session(_) => "session failure".into(),
        ErrorKind::BinaryNotFound { .. } => "runtime binary unavailable".into(),
        ErrorKind::InvalidConfig => "invalid SDK configuration".into(),
        ErrorKind::GitHubTokenProvider => "GitHub token provider failure".into(),
        _ => "unclassified SDK failure".into(),
    };
    // SDK messages and RPC data can contain credentials or response contents.
    format!("Model list is unavailable from Copilot. models.list: {detail}.")
}

pub(super) async fn refresh_commands(session: &Session, events: &EventQueue) {
    match super::commands::list_skill_commands(session).await {
        Ok(commands) => events.push(Event::Commands(commands)).await,
        Err(error) => events.push(Event::Error(error)).await,
    }
}

pub(super) async fn publish_sessions(client: &Client, events: &EventQueue) {
    match client.rpc().sessions().list().await {
        Ok(list) => {
            events
                .push(Event::Sessions(session_summaries(
                    list.sessions,
                    chrono::Utc::now(),
                )))
                .await
        }
        Err(_) => {
            events
                .push(Event::Error(
                    "Saved chat list is unavailable from Copilot.".into(),
                ))
                .await
        }
    }
}

pub(super) async fn refresh_agents(session: &Session, events: &EventQueue) {
    match session.rpc().agent().list().await {
        Ok(agents) => {
            events
                .push(Event::Agents(
                    agents
                        .agents
                        .into_iter()
                        .map(|agent| Choice {
                            id: agent.id,
                            label: agent.display_name,
                        })
                        .collect(),
                ))
                .await
        }
        Err(_) => {
            events
                .push(Event::Error(
                    "Agent list is unavailable from Copilot.".into(),
                ))
                .await
        }
    }
}

/// Chat output that does not come from the agent, such as a skill command's
/// own text.
pub(super) async fn note(events: &EventQueue, chat: ChatId, text: String) {
    events
        .push(Event::of(
            chat,
            ChatEvent::Entry(TranscriptEntry {
                id: format!("command-{}", super::next_id()),
                entry: Entry::Info(text),
            }),
        ))
        .await;
}

/// Replays saved SDK events through one mapper, so history produces exactly
/// the structure a live turn does: one row per tool call, not one per
/// execution event, and a call the turn never finished marked as such.
pub(super) fn history_entries(
    history: &[github_copilot_sdk::types::SessionEvent],
) -> Vec<TranscriptEntry> {
    replay(
        history
            .iter()
            .filter_map(|event| serde_json::to_value(event).ok()),
    )
}

/// The rows one journal produces, in the order the events arrived.
pub(super) fn replay(events: impl Iterator<Item = Value>) -> Vec<TranscriptEntry> {
    let mut transcript = Transcript::default();
    let mut entries: Vec<TranscriptEntry> = Vec::new();
    for event in events {
        let Some(row) = transcript.entry(&event) else {
            continue;
        };
        upsert_row(&mut entries, row);
    }
    for row in transcript.unstarted_rows() {
        upsert_row(&mut entries, row);
    }
    abort_unfinished(&mut entries);
    entries
}

/// A row the replay has seen before keeps its place and takes the newer
/// state, so a tool call's start and completion are one row.
fn upsert_row(entries: &mut Vec<TranscriptEntry>, row: TranscriptEntry) {
    match entries.iter_mut().find(|entry| entry.id == row.id) {
        Some(existing) => *existing = row,
        None => entries.push(row),
    }
}

/// A call still running at the end of a replay never finished: the turn was
/// aborted, or an approval it waited for never came.
fn abort_unfinished(entries: &mut [TranscriptEntry]) {
    for entry in entries.iter_mut() {
        if let Entry::Tool(call) = &mut entry.entry
            && call.status == ToolStatus::Running
        {
            call.status = ToolStatus::Aborted;
            call.result = UNFINISHED.to_owned();
        }
    }
}

/// The streamed text of one chunk, which extends the open row rather than
/// producing a confirmed one.
async fn bridge_delta(chat: ChatId, kind: &str, data: &Value, events: &EventQueue) -> bool {
    let entry = match kind {
        "assistant.message_delta" => EntryKind::Assistant,
        "assistant.reasoning_delta" => EntryKind::Thinking,
        _ => return false,
    };
    if let Some(text) = data["deltaContent"].as_str() {
        events.delta(chat, entry, text.into());
    }
    true
}

/// What the SDK reported for the last request.
fn usage(data: &Value) -> Usage {
    Usage {
        input_tokens: count(data, "inputTokens"),
        output_tokens: count(data, "outputTokens"),
        nano_aiu: data["copilotUsage"]["totalNanoAiu"].as_f64(),
    }
}

pub(super) async fn bridge_event(
    chat: ChatId,
    event: &Value,
    transcript: &mut Transcript,
    events: &EventQueue,
) {
    let data = &event["data"];
    let kind = event["type"].as_str().unwrap_or_default();
    // The mapper still sees reasoning, so its span covers the stream.
    if kind == "assistant.reasoning_delta" {
        transcript.entry(event);
    }
    if bridge_delta(chat, kind, data, events).await {
        return;
    }
    match kind {
        // The chat reports a `session.error` as a failure first, so this
        // only ends the turn.
        "session.idle" | "session.error" => events.push(Event::of(chat, ChatEvent::TurnDone)).await,
        "session.title_changed" => {
            if let Some(title) = data["title"].as_str() {
                events
                    .push(Event::of(chat, ChatEvent::Title(title.to_owned())))
                    .await;
            }
        }
        "assistant.usage" => {
            events
                .push(Event::of(chat, ChatEvent::Usage(usage(data))))
                .await;
        }
        _ => {
            if let Some(entry) = transcript.entry(event) {
                events.push(Event::of(chat, ChatEvent::Entry(entry))).await;
            }
        }
    }
}

pub(super) fn prompt(text: &str, context: &[ContextRef]) -> Result<String, String> {
    if text.trim().is_empty() {
        return Err("Enter a message before sending.".into());
    }
    let mut prompt = text.to_owned();
    let mut seen = HashSet::new();
    for item in context {
        if seen.insert(&item.key) {
            if contains_secret(&item.content) {
                return Err(
                    "Context contains credential-like data. Remove it before sending.".into(),
                );
            }
            prompt.push_str(crate::ATTACHMENT_MARKER);
            prompt.push_str(&item.label);
            prompt.push('\n');
            prompt.push_str(&item.content);
        }
        if prompt.len() > PROMPT_LIMIT {
            return Err("Message and context exceed 256 KiB.".into());
        }
    }
    if prompt.len() > PROMPT_LIMIT {
        return Err("Message and context exceed 256 KiB.".into());
    }
    Ok(prompt)
}

#[cfg(test)]
mod tests {
    use super::{model_list_error, replay};
    use crate::{Entry, ToolStatus};
    use github_copilot_sdk::{Error, ErrorKind};
    use serde_json::json;

    #[test]
    fn model_list_errors_keep_the_failure_class_without_sdk_payloads() {
        let cases = [
            (ErrorKind::Json, "invalid JSON request or response"),
            (ErrorKind::Io, "transport I/O failure"),
            (
                ErrorKind::Rpc { code: -32603 },
                "runtime RPC error (code -32603)",
            ),
            (ErrorKind::InvalidConfig, "invalid SDK configuration"),
            (
                ErrorKind::GitHubTokenProvider,
                "GitHub token provider failure",
            ),
        ];
        for (kind, detail) in cases {
            let error = Error::with_message(kind, "private response: Bearer secret-value");
            assert_eq!(
                model_list_error(&error),
                format!("Model list is unavailable from Copilot. models.list: {detail}.")
            );
        }
    }

    #[test]
    fn malformed_model_responses_are_reported_as_json_failures() {
        let error = serde_json::from_value::<github_copilot_sdk::rpc::ModelList>(json!({
            "models": [{"id": "test-model", "name": "Test model"}]
        }))
        .unwrap_err();
        assert_eq!(
            model_list_error(&Error::from(error)),
            "Model list is unavailable from Copilot. models.list: invalid JSON request or response."
        );
    }

    /// The journal of an aborted turn: the assistant asked for two calls, one
    /// started, neither completed, and no reply was ever written.
    fn aborted_turn() -> Vec<serde_json::Value> {
        vec![
            json!({"type": "user.message", "id": "u1", "data": {"content": "Count the risks"}}),
            json!({"type": "assistant.message", "id": "m1", "data": {
                "content": "",
                "toolRequests": [
                    {"toolCallId": "c1", "name": "subspace_read", "toolTitle": "Read services",
                     "arguments": {"action": "github_search"}},
                    {"toolCallId": "c2", "name": "subspace_read", "toolTitle": "Read services",
                     "arguments": {"action": "scope"}}
                ]
            }}),
            json!({"type": "tool.execution_start", "data": {"toolCallId": "c1", "toolName": "subspace_read"}}),
            json!({"type": "session.idle", "data": {}}),
        ]
    }

    #[test]
    fn a_replay_shows_every_tool_call_of_an_aborted_turn_once() {
        let rows = replay(aborted_turn().into_iter());
        assert_eq!(rows.len(), 3, "the message and both calls: {rows:?}");
        assert_eq!(
            rows[0].entry,
            Entry::User {
                text: "Count the risks".into(),
                steering: false
            }
        );
        let calls: Vec<(&str, ToolStatus)> = rows
            .iter()
            .filter_map(|row| row.entry.tool())
            .map(|call| (call.call_id.as_str(), call.status))
            .collect();
        assert_eq!(
            calls,
            vec![("c1", ToolStatus::Aborted), ("c2", ToolStatus::Aborted)],
            "a call that started but never completed is not left running"
        );
    }

    #[test]
    fn a_finished_call_is_one_row_carrying_its_result() {
        let mut events = aborted_turn();
        events.pop();
        events.push(json!({
            "type": "tool.execution_complete",
            "data": {"toolCallId": "c1", "success": true, "result": {"content": "3 risks"}}
        }));
        let rows = replay(events.into_iter());
        let first = rows[1].entry.tool().expect("a tool row");
        assert_eq!(first.status, ToolStatus::Done);
        assert_eq!(first.result, "3 risks");
        assert_eq!(
            rows.iter().filter(|row| row.entry.tool().is_some()).count(),
            2,
            "a start and its completion stay one row"
        );
    }
}
