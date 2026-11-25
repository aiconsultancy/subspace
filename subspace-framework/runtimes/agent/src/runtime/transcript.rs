//! Turns SDK session events into structured transcript rows.
//!
//! The mapper keeps the little state the SDK spreads across events: the tool
//! requests an `assistant.message` announced, which name and describe the
//! calls that follow, and the first and last timestamp of each `reasoningId`,
//! which give a thinking row its duration. Everything else is a pure function
//! of one event, so the whole mapping is testable from JSON.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::evidence::hosted_evidence;
use super::summary::{argument_summary, one_line};
use crate::{Entry, ToolCall, ToolStatus, TranscriptEntry, repeats};

/// The longest title a tool row shows, on one line.
const TITLE_LIMIT: usize = 80;
/// Milliseconds in a second, for the reasoning duration.
const MILLISECONDS: i64 = 1000;

/// What an `assistant.message` said about a call, kept until its execution
/// events arrive.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Request {
    title: String,
    detail: String,
}

/// First and last timestamp seen for one `reasoningId`, in milliseconds.
#[derive(Clone, Copy, Debug)]
struct Span {
    first: i64,
    last: i64,
}

/// What a row shows for a call the SDK never reported on.
pub(super) const UNFINISHED: &str = "This call did not finish: the turn ended first.";

#[derive(Default)]
pub(super) struct Transcript {
    requests: HashMap<String, Request>,
    reasoning: HashMap<String, Span>,
    /// Every call an assistant message asked for, in order.
    announced: Vec<String>,
    /// The calls an execution event has mentioned.
    started: HashSet<String>,
}

/// The most specific name the SDK offered for a call that does not simply
/// repeat its arguments. A tool whose intention summary is the query it runs
/// would otherwise show the same text twice, so the plain tool title is used
/// instead and the arguments are shown once.
pub(super) fn request_title(request: &Value, detail: &str) -> String {
    for field in ["intentionSummary", "toolTitle", "name"] {
        let Some(title) = request[field]
            .as_str()
            .filter(|text| !text.trim().is_empty())
        else {
            continue;
        };
        if repeats(title, detail) {
            continue;
        }
        return one_line(title, TITLE_LIMIT);
    }
    // Every name repeated the arguments, so the tool's own name is the title.
    let fallback = request["toolTitle"]
        .as_str()
        .or_else(|| request["name"].as_str())
        .unwrap_or("Tool");
    one_line(fallback, TITLE_LIMIT)
}

/// Milliseconds for an event timestamp, whether the SDK sent a number or an
/// RFC 3339 string.
fn timestamp_ms(event: &Value) -> Option<i64> {
    let timestamp = &event["timestamp"];
    if let Some(milliseconds) = timestamp.as_i64() {
        return Some(milliseconds);
    }
    chrono::DateTime::parse_from_rfc3339(timestamp.as_str()?)
        .ok()
        .map(|moment| moment.timestamp_millis())
}

fn reasoning_text(data: &Value) -> Option<String> {
    data["content"]
        .as_str()
        .or_else(|| data["reasoningText"].as_str())
        .map(ToOwned::to_owned)
}

/// The result or the error a completed call reported.
fn completion_result(data: &Value) -> String {
    data["error"]["message"]
        .as_str()
        .or_else(|| data["result"]["detailedContent"].as_str())
        .or_else(|| data["result"]["content"].as_str())
        .unwrap_or("No result details were supplied by the SDK.")
        .to_owned()
}

/// One row per `toolCallId`, so a start and its completion share an id and
/// the view updates the same row.
fn tool_id(call_id: &str) -> String {
    format!("tool:{call_id}")
}

/// A row that keeps the SDK's own event id.
/// The wire value `user.message` carries when Copilot injected the message
/// into a turn that was already running.
const STEERING: &str = "steering";

/// Zack's own message row. `delivery` is absent on older events and on
/// history entries, which then read as an ordinary message.
fn user_entry(data: &Value) -> Option<Entry> {
    Some(Entry::User {
        text: data["content"].as_str()?.to_owned(),
        steering: data["delivery"].as_str() == Some(STEERING),
    })
}

fn message_row(event: &Value, entry: Entry) -> TranscriptEntry {
    TranscriptEntry {
        id: event["id"].as_str().unwrap_or_default().to_owned(),
        entry,
    }
}

impl Transcript {
    /// Remembers what an `assistant.message` said about the calls it asked for.
    fn remember_requests(&mut self, data: &Value) {
        let Some(requests) = data["toolRequests"].as_array() else {
            return;
        };
        for request in requests {
            let Some(call_id) = request["toolCallId"].as_str() else {
                continue;
            };
            let detail = argument_summary(&request["arguments"]);
            self.requests.insert(
                call_id.to_owned(),
                Request {
                    title: request_title(request, &detail),
                    detail,
                },
            );
            self.announced.push(call_id.to_owned());
        }
    }

    /// The calls an `assistant.message` asked for that no execution event
    /// ever mentioned. A turn aborted before its tools ran leaves these, and
    /// a replay would otherwise lose them entirely.
    pub(super) fn unstarted_rows(&self) -> Vec<TranscriptEntry> {
        self.announced
            .iter()
            .filter(|call_id| !self.started.contains(*call_id))
            .map(|call_id| self.aborted_row(call_id))
            .collect()
    }

    fn aborted_row(&self, call_id: &str) -> TranscriptEntry {
        let request = self.requests.get(call_id).cloned().unwrap_or_default();
        TranscriptEntry {
            id: tool_id(call_id),
            entry: Entry::Tool(ToolCall {
                call_id: call_id.to_owned(),
                title: request.title,
                detail: request.detail,
                status: ToolStatus::Aborted,
                result: UNFINISHED.to_owned(),
            }),
        }
    }

    /// Extends the span of `reasoning_id` and returns its duration in whole
    /// seconds, if the events carried usable timestamps.
    fn reasoning_seconds(&mut self, reasoning_id: &str, event: &Value) -> Option<u64> {
        let now = timestamp_ms(event)?;
        let span = self
            .reasoning
            .entry(reasoning_id.to_owned())
            .or_insert(Span {
                first: now,
                last: now,
            });
        span.last = span.last.max(now);
        span.first = span.first.min(now);
        u64::try_from((span.last - span.first) / MILLISECONDS).ok()
    }

    /// The row for one call. A call no `assistant.message` announced still
    /// shows, named by the execution event's own tool name.
    fn tool_row(&self, call_id: &str, data: &Value, status: ToolStatus) -> TranscriptEntry {
        let request = self
            .requests
            .get(call_id)
            .cloned()
            .unwrap_or_else(|| Request {
                title: data["toolName"]
                    .as_str()
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or("Tool")
                    .to_owned(),
                detail: String::new(),
            });
        let result = match status {
            ToolStatus::Running => String::new(),
            _ => completion_result(data),
        };
        TranscriptEntry {
            id: tool_id(call_id),
            entry: Entry::Tool(ToolCall {
                call_id: call_id.to_owned(),
                title: request.title,
                detail: request.detail,
                status,
                result,
            }),
        }
    }

    /// The row an event produces, if it produces one. A reasoning delta only
    /// extends the span; an assistant message without text is dropped, so a
    /// turn that merely calls tools shows no empty row.
    pub(super) fn entry(&mut self, event: &Value) -> Option<TranscriptEntry> {
        let data = &event["data"];
        match event["type"].as_str()? {
            "user.message" => Some(message_row(event, user_entry(data)?)),
            "assistant.message" => {
                self.remember_requests(data);
                let content = data["content"].as_str().unwrap_or_default();
                let evidence = hosted_evidence(data);
                if content.trim().is_empty() && evidence.is_none() {
                    return None;
                }
                let text = match evidence {
                    Some(evidence) => format!(
                        "{}\n\nUntrusted web-search evidence:\n{evidence}",
                        content.trim_end()
                    ),
                    None => content.to_owned(),
                };
                Some(message_row(event, Entry::Assistant(text)))
            }
            "assistant.reasoning_delta" => {
                self.reasoning_seconds(data["reasoningId"].as_str().unwrap_or_default(), event);
                None
            }
            "assistant.reasoning" => {
                let text = reasoning_text(data)?;
                let seconds =
                    self.reasoning_seconds(data["reasoningId"].as_str().unwrap_or_default(), event);
                Some(message_row(event, Entry::Thinking { text, seconds }))
            }
            "tool.execution_start" => {
                let call_id = data["toolCallId"].as_str()?;
                self.started.insert(call_id.to_owned());
                Some(self.tool_row(call_id, data, ToolStatus::Running))
            }
            "tool.execution_complete" => {
                let call_id = data["toolCallId"].as_str()?;
                self.started.insert(call_id.to_owned());
                let status = if data["success"].as_bool() == Some(true) {
                    ToolStatus::Done
                } else {
                    ToolStatus::Failed
                };
                Some(self.tool_row(call_id, data, status))
            }
            "assistant.server_tool_progress" => {
                let kind = data["kind"].as_str().unwrap_or("hosted server tool");
                let status = data["status"].as_str().unwrap_or("in progress");
                Some(message_row(
                    event,
                    Entry::Info(format!("Untrusted {kind} progress: {status}")),
                ))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Transcript, argument_summary, one_line};
    use crate::{Entry, ToolStatus};
    use serde_json::json;

    fn at(seconds: i64) -> String {
        format!("2026-10-04T12:00:{seconds:02}Z")
    }

    #[test]
    fn scalar_arguments_read_as_one_line_led_by_the_action() {
        assert_eq!(
            argument_summary(&json!({"action": "github_search", "query": "is:open"})),
            "github_search query=is:open"
        );
        let fields = argument_summary(&json!({"limit": 20, "all": true}));
        assert!(fields.contains("limit=20"), "{fields}");
        assert!(
            fields.contains("all=true"),
            "every scalar field is summarised: {fields}"
        );
        assert_eq!(
            argument_summary(&json!({"action": "scope", "body": {"deep": 1}, "ids": [1, 2]})),
            "scope",
            "objects and arrays stay out of the one-line summary"
        );
        assert_eq!(argument_summary(&json!("not an object")), "");
        let long = argument_summary(&json!({"action": "x".repeat(200)}));
        assert_eq!(
            long.chars().count(),
            121,
            "cut to the limit plus an ellipsis"
        );
        assert!(long.ends_with('…'));
    }

    #[test]
    fn a_tool_call_is_one_row_named_by_its_request() {
        let mut transcript = Transcript::default();
        assert_eq!(
            transcript.entry(&json!({
                "type": "assistant.message",
                "id": "m1",
                "data": {
                    "content": "   ",
                    "toolRequests": [{
                        "toolCallId": "c1",
                        "name": "subspace_read",
                        "toolTitle": "Read services",
                        "intentionSummary": "Find the critical risks",
                        "arguments": {"action": "github_search", "query": "is:open"}
                    }]
                }
            })),
            None,
            "a turn that only calls tools shows no empty assistant row"
        );
        let start = transcript
            .entry(&json!({
                "type": "tool.execution_start",
                "data": {"toolCallId": "c1", "toolName": "subspace_read"}
            }))
            .expect("the start opens a row");
        let done = transcript
            .entry(&json!({
                "type": "tool.execution_complete",
                "data": {"toolCallId": "c1", "success": true, "result": {"content": "3 risks"}}
            }))
            .expect("the completion updates it");
        assert_eq!(start.id, done.id, "both events address one row");
        let Entry::Tool(call) = &done.entry else {
            panic!("expected a tool row, got {:?}", done.entry);
        };
        assert_eq!(call.call_id, "c1");
        assert_eq!(call.title, "Find the critical risks");
        assert_eq!(call.detail, "github_search query=is:open");
        assert_eq!(call.status, ToolStatus::Done);
        assert_eq!(call.result, "3 risks");
        assert_eq!(
            start.entry.tool().map(|call| call.status),
            Some(ToolStatus::Running)
        );
    }

    #[test]
    fn a_title_falls_back_and_a_failure_keeps_its_error() {
        let mut transcript = Transcript::default();
        transcript.entry(&json!({
            "type": "assistant.message",
            "id": "m1",
            "data": {"content": "Checking", "toolRequests": [
                {"toolCallId": "c1", "name": "subspace_read", "toolTitle": "Read services"},
                {"toolCallId": "c2", "name": "subspace_write"}
            ]}
        }));
        let titles = |transcript: &mut Transcript, call: &str| {
            let row = transcript
                .entry(&json!({
                    "type": "tool.execution_complete",
                    "data": {
                        "toolCallId": call,
                        "success": false,
                        "error": {"message": "Denied"}
                    }
                }))
                .expect("a completion is a row");
            let Entry::Tool(call) = row.entry else {
                panic!("expected a tool row");
            };
            call
        };
        let titled = titles(&mut transcript, "c1");
        assert_eq!(
            titled.title, "Read services",
            "the tool title names the call"
        );
        assert_eq!(titled.status, ToolStatus::Failed);
        assert_eq!(titled.result, "Denied");
        assert_eq!(
            titles(&mut transcript, "c2").title,
            "subspace_write",
            "the tool name is the last resort"
        );
        assert_eq!(
            titles(&mut transcript, "unknown").title,
            "Tool",
            "a call nothing announced still shows a row"
        );
    }

    #[test]
    fn hosted_search_progress_and_sources_are_untrusted_transcript_data() {
        let mut transcript = Transcript::default();
        let progress = transcript
            .entry(&json!({
                "type": "assistant.server_tool_progress",
                "id": "progress-1",
                "data": {"kind": "web_search", "status": "searching"}
            }))
            .expect("progress is visible");
        assert_eq!(
            progress.entry.text(),
            "Untrusted web_search progress: searching"
        );

        let reply = transcript
            .entry(&json!({
                "type": "assistant.message",
                "id": "reply-1",
                "data": {
                    "content": "Summary",
                    "serverTools": {"provider": "copilot", "items": [
                        {"title": "Example", "url": "https://example.test/result"}
                    ]}
                }
            }))
            .expect("hosted citations are visible");
        assert!(
            reply
                .entry
                .text()
                .contains("Untrusted web-search evidence:")
        );
        assert!(reply.entry.text().contains("https://example.test/result"));
    }

    #[test]
    fn thinking_spans_the_first_to_the_last_reasoning_event() {
        let mut transcript = Transcript::default();
        for second in [0, 7] {
            assert_eq!(
                transcript.entry(&json!({
                    "type": "assistant.reasoning_delta",
                    "timestamp": at(second),
                    "data": {"reasoningId": "r1", "deltaContent": "thinking"}
                })),
                None,
                "a delta only extends the span"
            );
        }
        let row = transcript
            .entry(&json!({
                "type": "assistant.reasoning",
                "id": "r-final",
                "timestamp": at(49),
                "data": {"reasoningId": "r1", "content": "I checked the risks"}
            }))
            .expect("the final reasoning event is a row");
        assert_eq!(
            row.entry,
            Entry::Thinking {
                text: "I checked the risks".into(),
                seconds: Some(49)
            }
        );
    }

    #[test]
    fn a_reasoning_event_without_a_usable_timestamp_has_no_duration() {
        let mut transcript = Transcript::default();
        let row = transcript
            .entry(&json!({
                "type": "assistant.reasoning",
                "id": "r1",
                "data": {"reasoningId": "r1", "reasoningText": "quick"}
            }))
            .expect("the row still appears");
        assert_eq!(
            row.entry,
            Entry::Thinking {
                text: "quick".into(),
                seconds: None
            }
        );
    }

    #[test]
    fn a_title_that_repeats_its_arguments_gives_way_to_the_tool_name() {
        let mut transcript = Transcript::default();
        transcript.entry(&json!({
            "type": "assistant.message",
            "id": "m1",
            "data": {"content": "", "toolRequests": [{
                "toolCallId": "c1",
                "name": "run_sql",
                "toolTitle": "Run query",
                "intentionSummary": "SELECT id FROM work_items WHERE state = 'Active'",
                "arguments": {"query": "SELECT id FROM work_items WHERE state = 'Active'"}
            }]}
        }));
        let row = transcript
            .entry(&json!({
                "type": "tool.execution_start",
                "data": {"toolCallId": "c1", "toolName": "run_sql"}
            }))
            .expect("a row");
        let Entry::Tool(call) = row.entry else {
            panic!("expected a tool row");
        };
        assert_eq!(
            call.title, "Run query",
            "the query is shown once, in the detail"
        );
        assert_eq!(
            call.detail,
            "query=SELECT id FROM work_items WHERE state = 'Active'"
        );
    }

    #[test]
    fn a_long_title_is_one_line_and_cut() {
        assert_eq!(one_line("  keep   this\n line  ", 80), "keep this line");
        let long = one_line(&"word ".repeat(40), 80);
        assert_eq!(long.chars().count(), 81);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn a_turn_aborted_before_its_tools_ran_still_shows_them() {
        let mut transcript = Transcript::default();
        transcript.entry(&json!({
            "type": "assistant.message",
            "id": "m1",
            "data": {"content": "", "toolRequests": [
                {"toolCallId": "c1", "name": "subspace_read", "toolTitle": "Read services",
                 "arguments": {"action": "github_search"}},
                {"toolCallId": "c2", "name": "subspace_read", "toolTitle": "Read services",
                 "arguments": {"action": "scope"}}
            ]}
        }));
        // Only the first call ever started, and nothing completed.
        transcript.entry(&json!({
            "type": "tool.execution_start",
            "data": {"toolCallId": "c1", "toolName": "subspace_read"}
        }));
        let unstarted = transcript.unstarted_rows();
        assert_eq!(unstarted.len(), 1, "the call that never ran is reported");
        let Entry::Tool(call) = &unstarted[0].entry else {
            panic!("expected a tool row");
        };
        assert_eq!(call.call_id, "c2");
        assert_eq!(call.status, ToolStatus::Aborted);
        assert_eq!(call.detail, "scope");
        assert!(call.result.contains("did not finish"));
    }

    #[test]
    fn a_message_delivered_while_the_turn_ran_is_marked_as_steering() {
        let mut transcript = Transcript::default();
        let steered = transcript
            .entry(&json!({
                "type": "user.message",
                "id": "u2",
                "data": {"content": "use the other branch", "delivery": "steering"}
            }))
            .expect("a user message is a row");
        assert_eq!(
            steered.entry,
            Entry::User {
                text: "use the other branch".into(),
                steering: true
            }
        );
        let queued = transcript
            .entry(&json!({
                "type": "user.message",
                "id": "u3",
                "data": {"content": "and then stop", "delivery": "queued"}
            }))
            .expect("a user message is a row");
        assert_eq!(
            queued.entry.kind(),
            crate::EntryKind::User,
            "only steering carries the label"
        );
        assert_eq!(
            queued.entry,
            Entry::User {
                text: "and then stop".into(),
                steering: false
            }
        );
    }

    #[test]
    fn messages_and_unknown_events_map_as_expected() {
        let mut transcript = Transcript::default();
        let user = transcript
            .entry(&json!({
                "type": "user.message",
                "id": "u1",
                "data": {"content": "Review this"}
            }))
            .expect("a user message is a row");
        assert_eq!(user.id, "u1");
        assert_eq!(
            user.entry,
            Entry::User {
                text: "Review this".into(),
                steering: false
            }
        );
        assert_eq!(
            transcript
                .entry(&json!({
                    "type": "assistant.message",
                    "id": "a1",
                    "data": {"content": "Done"}
                }))
                .map(|row| row.entry),
            Some(Entry::Assistant("Done".into()))
        );
        assert_eq!(transcript.entry(&json!({"type": "session.idle"})), None);
        assert_eq!(transcript.entry(&json!({"data": {}})), None);
    }

    #[test]
    fn numeric_timestamps_also_give_a_duration() {
        let mut transcript = Transcript::default();
        transcript.entry(&json!({
            "type": "assistant.reasoning_delta",
            "timestamp": 1_000_i64,
            "data": {"reasoningId": "r1"}
        }));
        let row = transcript
            .entry(&json!({
                "type": "assistant.reasoning",
                "timestamp": 4_000_i64,
                "data": {"reasoningId": "r1", "content": "ok"}
            }))
            .expect("a row");
        assert_eq!(
            row.entry,
            Entry::Thinking {
                text: "ok".into(),
                seconds: Some(3)
            }
        );
    }
}
