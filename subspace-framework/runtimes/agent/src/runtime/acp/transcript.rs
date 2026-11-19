//! `session/update` notifications as transcript rows. Streamed text goes out
//! as deltas and is confirmed as one row when the next kind of update starts,
//! so a confirmed row always lands where its stream did.
use super::{Entry, EntryKind, HashMap, ToolStatus, TranscriptEntry, Value};
use crate::ToolCall;
use crate::runtime::transcript;

const TITLE_LIMIT: usize = 120;

/// What one update changes in the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Output {
    Delta(EntryKind, String),
    Entry(TranscriptEntry),
}

/// The text being streamed into one row.
#[derive(Clone)]
struct Segment {
    id: String,
    kind: EntryKind,
    message: Option<String>,
    text: String,
}

/// One chat's rows, built from its updates. Row ids are unique per chat.
#[derive(Clone, Default)]
pub(super) struct AcpTranscript {
    rows: u64,
    generation: u64,
    open: Option<Segment>,
    tools: HashMap<String, ToolCall>,
    plan: Option<String>,
}

impl AcpTranscript {
    /// A replacement process may reuse tool ids; historical rows keep theirs.
    pub(super) fn restart(&mut self) {
        self.generation += 1;
        self.tools.clear();
        self.plan = None;
    }

    /// Applies one `update`. User chunks only count in a `session/load`
    /// replay; a live turn shows the message Subspace sent.
    pub(super) fn apply(&mut self, update: &Value, replay: bool) -> Vec<Output> {
        match update["sessionUpdate"].as_str().unwrap_or_default() {
            "agent_message_chunk" => self.chunk(EntryKind::Assistant, update),
            "agent_thought_chunk" => self.chunk(EntryKind::Thinking, update),
            "user_message_chunk" if replay => self.chunk(EntryKind::User, update),
            "tool_call" => self.tool(update, true),
            "tool_call_update" => self.tool(update, false),
            "plan" => self.plan(update),
            _ => Vec::new(),
        }
    }

    /// The message Subspace sent, as its own row.
    pub(super) fn user(&mut self, text: &str) -> Vec<Output> {
        let mut outputs = self.close();
        let id = self.next_id("user");
        outputs.push(Output::Entry(TranscriptEntry {
            id,
            entry: Entry::User {
                text: text.to_owned(),
                steering: false,
            },
        }));
        outputs
    }

    /// Ends a turn: confirms the open text and, when the turn was stopped,
    /// marks the calls that never finished.
    pub(super) fn finish(&mut self, stopped: bool) -> Vec<Output> {
        let mut outputs = self.close();
        self.plan = None;
        let unfinished = self
            .tools
            .values_mut()
            .filter(|call| stopped && call.status == ToolStatus::Running);
        for call in unfinished {
            call.status = ToolStatus::Aborted;
            outputs.push(tool_output(call, self.generation));
        }
        outputs
    }

    fn next_id(&mut self, kind: &str) -> String {
        self.rows += 1;
        format!("acp:{}:{kind}", self.rows)
    }

    fn chunk(&mut self, kind: EntryKind, update: &Value) -> Vec<Output> {
        let content = &update["content"];
        let Some(text) = content["text"]
            .as_str()
            .filter(|_| content["type"].as_str() == Some("text"))
            .and_then(|text| shown_text(&kind, text))
        else {
            return Vec::new();
        };
        let message = update["messageId"].as_str().map(str::to_owned);
        let continues = self
            .open
            .as_ref()
            .is_some_and(|open| open.kind == kind && open.message == message);
        let mut outputs = if continues { Vec::new() } else { self.close() };
        if !continues {
            let id = self.next_id("text");
            self.open = Some(Segment {
                id,
                kind: kind.clone(),
                message,
                text: String::new(),
            });
        }
        if let Some(open) = self.open.as_mut() {
            open.text.push_str(text);
        }
        outputs.push(Output::Delta(kind, text.to_owned()));
        outputs
    }

    /// Confirms the streamed row, if one is open.
    fn close(&mut self) -> Vec<Output> {
        let Some(segment) = self.open.take() else {
            return Vec::new();
        };
        let entry = match segment.kind {
            EntryKind::User => Entry::User {
                text: segment.text,
                steering: false,
            },
            EntryKind::Thinking => Entry::Thinking {
                text: segment.text,
                seconds: None,
            },
            _ => Entry::Assistant(segment.text),
        };
        vec![Output::Entry(TranscriptEntry {
            id: segment.id,
            entry,
        })]
    }

    fn tool(&mut self, update: &Value, started: bool) -> Vec<Output> {
        let Some(id) = update["toolCallId"].as_str() else {
            return Vec::new();
        };
        let mut outputs = self.close();
        let call = self
            .tools
            .entry(id.to_owned())
            .or_insert_with(|| new_call(id));
        merge_call(call, update, started);
        outputs.push(tool_output(call, self.generation));
        outputs
    }

    fn plan(&mut self, update: &Value) -> Vec<Output> {
        let mut outputs = self.close();
        let id = match self.plan.clone() {
            Some(id) => id,
            None => {
                let id = self.next_id("plan");
                self.plan = Some(id.clone());
                id
            }
        };
        outputs.push(Output::Entry(TranscriptEntry {
            id,
            entry: Entry::Info(plan_text(&update["entries"])),
        }));
        outputs
    }
}

/// A replayed user message shows what the user wrote, without the context
/// block Subspace sent before it; a chunk of only that block shows nothing.
fn shown_text<'a>(kind: &EntryKind, text: &'a str) -> Option<&'a str> {
    match kind {
        EntryKind::User => {
            Some(super::context::without_context(text)).filter(|text| !text.is_empty())
        }
        _ => Some(text),
    }
}

fn new_call(id: &str) -> ToolCall {
    ToolCall {
        call_id: id.to_owned(),
        title: "Tool".to_owned(),
        detail: String::new(),
        status: ToolStatus::Running,
        result: String::new(),
    }
}

fn tool_output(call: &ToolCall, generation: u64) -> Output {
    Output::Entry(TranscriptEntry {
        id: format!("tool:{generation}:{}", call.call_id),
        entry: Entry::Tool(call.clone()),
    })
}

/// A `tool_call` sets every field; a `tool_call_update` changes the fields it
/// carries, and its `content` replaces the earlier content.
fn merge_call(call: &mut ToolCall, update: &Value, started: bool) {
    let title = update["title"]
        .as_str()
        .or_else(|| update["name"].as_str())
        .or_else(|| update["kind"].as_str())
        .filter(|title| !title.trim().is_empty());
    if let Some(title) = title {
        call.title = transcript::one_line(title, TITLE_LIMIT);
    }
    if update.get("rawInput").is_some() || started {
        call.detail = transcript::argument_summary(&update["rawInput"]);
    }
    if let Some(status) = update["status"].as_str().map(tool_status) {
        call.status = status;
    }
    if let Some(content) = update["content"].as_array() {
        call.result = content_text(content);
    }
}

fn tool_status(status: &str) -> ToolStatus {
    match status {
        "completed" => ToolStatus::Done,
        "failed" => ToolStatus::Failed,
        _ => ToolStatus::Running,
    }
}

/// The readable part of a call's content: its text, and a line for each diff
/// or terminal it refers to.
fn content_text(content: &[Value]) -> String {
    content
        .iter()
        .filter_map(|item| match item["type"].as_str()? {
            "content" => item["content"]["text"].as_str().map(str::to_owned),
            "diff" => Some(format!(
                "Edited {}",
                item["path"].as_str().unwrap_or("a file")
            )),
            "terminal" => Some("Ran a command in the agent's terminal.".to_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn plan_text(entries: &Value) -> String {
    let steps = entries.as_array().into_iter().flatten().map(|entry| {
        let mark = match entry["status"].as_str() {
            Some("completed") => "✓",
            Some("in_progress") => "→",
            _ => "·",
        };
        format!("{mark} {}", entry["content"].as_str().unwrap_or_default())
    });
    std::iter::once("Plan".to_owned())
        .chain(steps)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Folds confirmed rows into a transcript, a later row replacing its id.
pub(super) fn fold(rows: &mut Vec<TranscriptEntry>, outputs: &[Output]) {
    for output in outputs {
        let Output::Entry(entry) = output else {
            continue;
        };
        match rows.iter_mut().find(|row| row.id == entry.id) {
            Some(row) => *row = entry.clone(),
            None => rows.push(entry.clone()),
        }
    }
}
