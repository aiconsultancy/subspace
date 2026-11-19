//! The `data:` lines of a Copilot session log, parsed into entries.
//!
//! The stream is undocumented, so every line is read defensively: a line
//! that does not parse, or that carries a shape this module does not know,
//! is skipped rather than failing the log.
//!
//! Each line is a whole message, not a piece of one. A chunk that carries
//! tool calls and no text is the agent asking for those calls; the same
//! chunk shape with text is that call's answer, which also arrives as a
//! `{"role": "tool"}` line. Both the repeated call and the repeated answer
//! are dropped, so one call and one answer make one row each.

use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash as _, Hasher as _};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One entry of a parsed session log, in the order the log holds them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogEntry {
    /// What the agent said, as Markdown.
    Assistant { created: Option<i64>, text: String },
    /// A tool the agent called, with the arguments it sent. `id` is the
    /// call id the stream gives.
    ToolCall {
        created: Option<i64>,
        id: String,
        name: String,
        arguments: String,
    },
    /// A message from the user or from the harness.
    User { text: String },
    /// What a tool answered. `name` is empty when the answer arrives
    /// before, or without, the call it belongs to.
    ToolResult { name: String, text: String },
}

/// A parsed session log.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionLog {
    pub entries: Vec<LogEntry>,
    /// The read stopped at the byte cap, so the log is the start only.
    pub truncated: bool,
}

/// What one `data:` line may hold. Every field is optional: the known
/// shapes share one document type, so an unknown mix still reads.
#[derive(Debug, Default, Deserialize)]
struct RawLine {
    #[serde(default)]
    created: Option<i64>,
    #[serde(default)]
    choices: Vec<RawChoice>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    content: Option<Value>,
}

#[derive(Debug, Default, Deserialize)]
struct RawChoice {
    #[serde(default)]
    delta: RawDelta,
}

#[derive(Debug, Default, Deserialize)]
struct RawDelta {
    #[serde(default)]
    content: Option<Value>,
    #[serde(default)]
    tool_calls: Vec<RawToolCall>,
}

#[derive(Debug, Deserialize)]
struct RawToolCall {
    #[serde(default)]
    id: String,
    #[serde(default)]
    function: Option<RawFunction>,
}

#[derive(Debug, Deserialize)]
struct RawFunction {
    #[serde(default)]
    name: String,
    #[serde(default)]
    arguments: String,
}

/// The `data:` payloads of an event stream. A truncated read loses its last
/// line, which is the one the cap cut in half.
fn data_lines(text: &str, truncated: bool) -> Vec<&str> {
    let mut lines: Vec<&str> = text
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .filter(|line| !line.is_empty() && *line != "[DONE]")
        .collect();
    if truncated {
        lines.pop();
    }
    lines
}

/// Message content, which is either a plain string or a list of parts.
fn content_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<&str>>()
            .join(""),
        _ => String::new(),
    }
}

fn digest(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// The call's name and arguments, which the stream repeats in full.
fn function_of(call: &RawToolCall) -> (&str, &str) {
    match call.function.as_ref() {
        Some(function) => (function.name.as_str(), function.arguments.as_str()),
        None => ("", ""),
    }
}

/// The log as it is read: the entries so far, the calls already asked for,
/// and the answers already shown.
#[derive(Default)]
struct Reading {
    entries: Vec<LogEntry>,
    calls: Vec<(String, String)>,
    answered: HashSet<u64>,
}

impl Reading {
    /// The name of the call `id` asked for, when that call was seen.
    fn name_of(&self, id: &str) -> String {
        self.calls
            .iter()
            .find(|(known, _)| known == id)
            .map(|(_, name)| name.clone())
            .unwrap_or_default()
    }

    /// A call the agent asked for. A repeat of a call already asked for is
    /// dropped: the stream sends each one again with its answer.
    fn call(&mut self, created: Option<i64>, call: &RawToolCall) {
        let (name, arguments) = function_of(call);
        if name.is_empty() || self.calls.iter().any(|(known, _)| known == &call.id) {
            return;
        }
        self.calls.push((call.id.clone(), name.to_owned()));
        self.entries.push(LogEntry::ToolCall {
            created,
            id: call.id.clone(),
            name: name.to_owned(),
            arguments: arguments.to_owned(),
        });
    }

    /// An answer, from a result chunk or from a `role: tool` line. The same
    /// text is shown once.
    fn answer(&mut self, name: String, text: String) {
        if text.is_empty() || !self.answered.insert(digest(&text)) {
            return;
        }
        self.entries.push(LogEntry::ToolResult { name, text });
    }

    fn said(&mut self, created: Option<i64>, text: String) {
        if !text.is_empty() {
            self.entries.push(LogEntry::Assistant { created, text });
        }
    }

    /// One `chat.completion.chunk`: tool calls with no text are what the
    /// agent asked for, tool calls with text are an answer, and text on its
    /// own is what the agent said.
    fn chunk(&mut self, line: &RawLine) {
        for choice in &line.choices {
            let text = content_text(choice.delta.content.as_ref());
            let calls = &choice.delta.tool_calls;
            match (calls.first(), text.is_empty()) {
                (Some(first), false) => self.answer(self.name_of(&first.id), text),
                (Some(_), true) => {
                    for call in calls {
                        self.call(line.created, call);
                    }
                }
                (None, _) => self.said(line.created, text),
            }
        }
    }

    /// Adds what one line holds, or nothing when it does not parse.
    fn line(&mut self, payload: &str) {
        let Ok(line) = serde_json::from_str::<RawLine>(payload) else {
            return;
        };
        let text = content_text(line.content.as_ref());
        match line.role.as_deref() {
            Some("user") if !text.is_empty() => self.entries.push(LogEntry::User { text }),
            Some("user") => {}
            Some("tool") => self.answer(String::new(), text),
            _ => self.chunk(&line),
        }
    }
}

/// The parsed log of an event stream. `truncated` says the read hit its cap.
pub fn parse_log(text: &str, truncated: bool) -> SessionLog {
    let mut reading = Reading::default();
    for payload in data_lines(text, truncated) {
        reading.line(payload);
    }
    SessionLog {
        entries: reading.entries,
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::{LogEntry, SessionLog, parse_log};

    const FIXTURE: &str = include_str!("../../tests/fixtures/session-log.txt");

    fn names(log: &SessionLog) -> Vec<&str> {
        log.entries
            .iter()
            .map(|entry| match entry {
                LogEntry::Assistant { .. } => "assistant",
                LogEntry::ToolCall { .. } => "tool-call",
                LogEntry::User { .. } => "user",
                LogEntry::ToolResult { .. } => "tool-result",
            })
            .collect()
    }

    #[test]
    fn the_known_shapes_become_entries_in_order() {
        let log = parse_log(FIXTURE, false);
        assert!(!log.truncated);
        assert_eq!(
            names(&log),
            ["user", "assistant", "tool-call", "tool-result", "assistant"]
        );
        assert_eq!(
            log.entries[0],
            LogEntry::User {
                text: "fix the login button".into()
            }
        );
        assert_eq!(
            log.entries[1],
            LogEntry::Assistant {
                created: Some(1_790_000_000),
                text: "Looking at the login button.".into()
            }
        );
    }

    #[test]
    fn a_call_the_stream_repeats_with_its_answer_is_one_row() {
        let log = parse_log(FIXTURE, false);
        assert_eq!(
            log.entries[2],
            LogEntry::ToolCall {
                created: Some(1_790_000_002),
                id: "call_1".into(),
                name: "bash".into(),
                arguments: r#"{"command":"ls"}"#.into()
            }
        );
        assert_eq!(
            log.entries[3],
            LogEntry::ToolResult {
                name: "bash".into(),
                text: "README.md\nsrc\n".into()
            },
            "the answer chunk and the role:tool line are the same answer"
        );
    }

    #[test]
    fn text_parts_are_joined_and_a_second_answer_is_still_shown() {
        let log = parse_log(FIXTURE, false);
        assert_eq!(
            log.entries[4],
            LogEntry::Assistant {
                created: Some(1_790_000_004),
                text: "Done: the first part and the second part.".into()
            }
        );
    }

    #[test]
    fn an_answer_without_its_call_is_shown_without_a_tool_name() {
        let text = "data: {\"role\":\"tool\",\"tool_call_id\":\"call_9\",\"content\":\"done\"}\n";
        assert_eq!(
            parse_log(text, false).entries,
            vec![LogEntry::ToolResult {
                name: String::new(),
                text: "done".into()
            }]
        );
    }

    #[test]
    fn unknown_broken_and_empty_lines_are_skipped() {
        let text = concat!(
            "event: ping\n",
            "data: not json\n",
            "data: {\"object\":\"something.new\",\"what\":1}\n",
            "data: [DONE]\n",
            "\n",
            "data: {\"role\":\"system\",\"content\":\"\"}\n",
        );
        assert!(parse_log(text, false).entries.is_empty());
    }

    #[test]
    fn a_truncated_read_drops_its_half_line_and_says_so() {
        let text = concat!(
            "data: {\"role\":\"user\",\"content\":\"hello\"}\n",
            "data: {\"role\":\"user\",\"conte",
        );
        let log = parse_log(text, true);
        assert!(log.truncated);
        assert_eq!(names(&log), ["user"]);
        assert_eq!(parse_log(text, false).entries.len(), 1, "half line skipped");
    }
}
