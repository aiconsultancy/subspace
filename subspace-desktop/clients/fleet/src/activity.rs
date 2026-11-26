//! An officer's turns, as `GET /api/v1/agents/{id}/activity` and the
//! `agent-activity` event give them: model steps with settled tokens and
//! cost, tool calls with their full input, progress, output and errors,
//! and reply and reasoning text.
//!
//! Stargate neither redacts nor truncates this route, and nor does this
//! client: an item too large for one answer is read page by page and
//! joined in full, whatever its size, when its reader can hold it, or
//! else read part by part ([`crate::FleetClient::activity_item`]). A known
//! kind or shape keeps every field this version does not know (`extra`),
//! so nothing sent is lost. The content is kept in memory only. Its types deliberately do not implement
//! `Debug`, so no log line or panic message can carry what a tool printed.

use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

mod join;
mod read;

pub use join::{ActivityJoin, ActivityNews, Fetch};
pub(crate) use join::{MAX_HELD_BYTES, MAX_HELD_EVENTS};
pub use read::{ItemPart, ItemRead, PartFailed};
#[cfg(test)]
use read::{Joined, decode_item, join_pages, read_part};

/// `V1AgentActivity`: an officer and its recent turns, oldest first.
#[derive(Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct AgentActivity {
    pub agent: ActivityAgent,
    pub turns: Vec<ActivityTurn>,
    pub cursor: ActivityCursor,
}

#[derive(Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ActivityAgent {
    /// The registration ID.
    pub id: String,
    pub role: crate::model::Role,
    pub backend: String,
    /// False while this gateway does not collect turns for the backend.
    pub collected: bool,
}

/// Changes up to `seq` are included; `generation` changes on restart.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct ActivityCursor {
    pub generation: String,
    pub seq: u64,
}

/// A turn's state.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TurnStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
    /// A state this version does not know.
    #[default]
    #[serde(other)]
    Unknown,
}

/// `V1ActivityTurnSummary`: a turn without its items.
#[derive(Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TurnSummary {
    pub id: String,
    pub seq: u64,
    #[serde(default)]
    pub status: TurnStatus,
    pub started_at: i64,
    pub updated_at: i64,
    pub ended_at: Option<i64>,
    /// Times the live provider stream reattached: changes can be missing.
    #[serde(default)]
    pub reconnects: u32,
    /// Null until a step settles.
    pub usage: Option<ActivityUsage>,
}

/// `V1ActivityTurn`: a turn and its items in first-observed order.
#[derive(Clone, Deserialize, PartialEq)]
#[non_exhaustive]
pub struct ActivityTurn {
    #[serde(flatten)]
    pub summary: TurnSummary,
    pub items: Vec<ActivityEntry>,
}

/// Settled use summed over a turn's steps.
#[derive(Clone, Deserialize, PartialEq)]
#[non_exhaustive]
pub struct ActivityUsage {
    pub steps: u32,
    pub tokens: ActivityTokens,
    /// USD.
    pub cost: f64,
}

#[derive(Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct ActivityTokens {
    pub input: u64,
    pub output: u64,
    pub reasoning: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

/// What every item carries.
#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ItemBase {
    /// Stable within its turn.
    pub id: String,
    /// The agent sequence of the item's latest change.
    pub seq: u64,
    pub started_at: i64,
    pub updated_at: i64,
}

/// One item of a turn: in full, only its identity when it is too large to
/// send inline, or of a kind this version does not know.
#[derive(Clone, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ActivityEntry {
    /// Read its pages to see it ([`crate::FleetClient::activity_item`]).
    Paged(PagedItem),
    Item(Box<ActivityItem>),
    /// A kind this version does not draw; kept so its ID and sequence
    /// still count.
    Unknown(UnknownItem),
}

impl ActivityEntry {
    pub fn base(&self) -> &ItemBase {
        match self {
            Self::Paged(item) => &item.base,
            Self::Item(item) => item.base(),
            Self::Unknown(item) => &item.base,
        }
    }
}

/// `V1ActivityItem`, by its `kind`.
#[derive(Clone, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ActivityItem {
    Step(StepItem),
    Text(TextItem),
    Reasoning(TextItem),
    Tool(ToolItem),
}

impl ActivityItem {
    pub fn base(&self) -> &ItemBase {
        match self {
            Self::Step(item) => &item.base,
            Self::Text(item) | Self::Reasoning(item) => &item.base,
            Self::Tool(item) => &item.base,
        }
    }
}

/// One model request and response.
#[derive(Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct StepItem {
    #[serde(flatten)]
    pub base: ItemBase,
    pub status: String,
    pub model: Option<StepModel>,
    pub first_token_at: Option<i64>,
    pub streamed_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub finish: Option<String>,
    /// Settled token use; `None` while running.
    pub tokens: Option<ActivityTokens>,
    /// Settled cost in USD; `None` while running or when unpriced.
    pub cost: Option<f64>,
    pub error: Option<ActivityError>,
    /// Fields this version does not know, kept so they can still be shown.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct StepModel {
    pub provider: String,
    pub id: String,
    pub variant: Option<String>,
    /// Fields this version does not know, kept so they can still be shown.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Reply text, or the model's reasoning text.
#[derive(Clone, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct TextItem {
    #[serde(flatten)]
    pub base: ItemBase,
    /// `streaming` or `completed`.
    pub status: String,
    pub text: String,
    /// Fields this version does not know, kept so they can still be shown.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One tool call.
#[derive(Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ToolItem {
    #[serde(flatten)]
    pub base: ItemBase,
    /// The provider's tool call ID.
    pub call_id: String,
    /// `None` until the call's name is observed.
    pub name: Option<String>,
    /// `input`, `running`, `completed` or `failed`.
    pub status: String,
    /// The arguments as the model streamed them.
    #[serde(default)]
    pub raw_input: String,
    /// The parsed arguments once the call was made.
    pub input: Option<Value>,
    pub executed: Option<bool>,
    /// The latest progress or result metadata, such as live output.
    pub metadata: Option<Value>,
    /// The result, or a failed call's partial output.
    pub content: Option<Vec<ActivityContent>>,
    pub error: Option<ActivityError>,
    /// Fields this version does not know, kept so they can still be shown.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One part of a tool's output. A known type keeps the fields this version
/// does not know in `extra`, so nothing sent is lost.
#[derive(Clone, PartialEq, Eq)]
pub enum ActivityContent {
    Text {
        text: String,
        extra: Map<String, Value>,
    },
    /// `uri` is often a `data:` URI holding the whole file.
    File {
        uri: String,
        mime: String,
        name: Option<String>,
        extra: Map<String, Value>,
    },
    /// A content type this version does not know, or a known one in a
    /// shape it does not know, kept whole so it can still be shown.
    Other(Value),
}

impl<'de> Deserialize<'de> for ActivityContent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "lowercase")]
        enum Known {
            Text {
                text: String,
                #[serde(flatten)]
                extra: Map<String, Value>,
            },
            File {
                uri: String,
                mime: String,
                name: Option<String>,
                #[serde(flatten)]
                extra: Map<String, Value>,
            },
        }
        let raw = Value::deserialize(deserializer)?;
        Ok(match Known::deserialize(&raw) {
            Ok(Known::Text { text, extra }) => Self::Text { text, extra },
            Ok(Known::File {
                uri,
                mime,
                name,
                extra,
            }) => Self::File {
                uri,
                mime,
                name,
                extra,
            },
            Err(_) => Self::Other(raw),
        })
    }
}

/// A provider failure as the agent saw it.
#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ActivityError {
    #[serde(rename = "type")]
    pub kind: String,
    pub message: String,
    pub status: Option<u16>,
    pub response_body: Option<String>,
    pub retry_after_ms: Option<u64>,
    /// Fields this version does not know, kept so they can still be shown.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// An item whose JSON exceeds 64 KiB: its identity and how to read it.
#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PagedItem {
    #[serde(flatten)]
    pub base: ItemBase,
    pub kind: String,
    pub status: String,
    pub call_id: Option<String>,
    pub name: Option<String>,
    pub paged: Paging,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct Paging {
    pub pages: u32,
    pub bytes: u64,
}

/// An item of a kind this version does not know, or of a known kind in a
/// shape it does not know, kept whole so it can still be shown.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct UnknownItem {
    pub base: ItemBase,
    pub kind: String,
    /// The item's JSON as sent.
    pub raw: Value,
}

impl<'de> Deserialize<'de> for UnknownItem {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Shape {
            #[serde(flatten)]
            base: ItemBase,
            kind: String,
        }
        let raw = Value::deserialize(deserializer)?;
        let shape = Shape::deserialize(&raw)
            .map_err(|_| serde::de::Error::custom("an item without an ID, seq or kind"))?;
        Ok(Self {
            base: shape.base,
            kind: shape.kind,
            raw,
        })
    }
}

/// `V1ActivityItemPage`: one page of one item's complete JSON.
#[derive(Clone, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct ItemPage {
    pub agent: String,
    pub turn: String,
    pub item: String,
    /// Pages of different `seq` values do not join.
    pub seq: u64,
    pub page: u32,
    pub pages: u32,
    /// The UTF-8 length of the whole item JSON.
    pub bytes: u64,
    pub data: String,
}

impl ItemPage {
    /// The version of the item this page belongs to.
    pub fn version(&self) -> ItemVersion {
        ItemVersion {
            seq: self.seq,
            pages: self.pages,
            bytes: self.bytes,
        }
    }
}

/// What tells one version of a paged item from another: its `seq`, page
/// count and total bytes. Pages of different versions do not join, and a
/// part read is pinned to one ([`crate::FleetClient::activity_item_part`])
/// without holding a copy of any page.
#[derive(Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ItemVersion {
    pub seq: u64,
    pub pages: u32,
    pub bytes: u64,
}

/// `V1AgentActivityEvent`: one coalesced change to an officer's turns.
#[derive(Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct AgentActivityEvent {
    pub agent_id: String,
    pub turn_id: String,
    pub generation: String,
    pub seq: u64,
    pub at: i64,
    pub change: ActivityChange,
    /// The bytes of JSON the event was read from, which bound what is
    /// held of an officer's events; 0 when not read off a stream.
    #[serde(skip)]
    pub(crate) bytes: usize,
}

/// Names only what identifies the change, never its content.
impl std::fmt::Debug for AgentActivityEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentActivityEvent")
            .field("agent_id", &self.agent_id)
            .field("turn_id", &self.turn_id)
            .field("generation", &self.generation)
            .field("seq", &self.seq)
            .finish_non_exhaustive()
    }
}

/// An upsert by ID: a turn's summary or one item.
#[derive(Clone, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ActivityChange {
    TurnStarted {
        turn: TurnSummary,
    },
    /// The live provider stream reattached.
    TurnUpdated {
        turn: TurnSummary,
    },
    /// With its outcome, `endedAt` and settled usage.
    TurnEnded {
        turn: TurnSummary,
    },
    Item {
        item: Box<ActivityEntry>,
    },
}

#[cfg(test)]
mod tests;
