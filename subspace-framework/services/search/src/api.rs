use serde::{Deserialize, Serialize};

const MAX_REFERENCE_PART_BYTES: usize = 512;
const MAX_REFERENCE_BYTES: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    pub app: String,
    pub board: String,
    pub endpoint: String,
    pub tenant: String,
    pub account: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub title: String,
    pub body: String,
    pub metadata: String,
    pub parent_id: Option<String>,
    pub kind: String,
    pub url: Option<String>,
    pub navigation: String,
    pub refreshed_at: i64,
    pub payload: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub scope: Scope,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hit {
    pub target: Target,
    pub document: Document,
    pub excerpt: String,
    pub score: f64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    pub enabled: bool,
    pub retention_days: u32,
    pub max_bytes: u64,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            enabled: false,
            retention_days: 60,
            max_bytes: 2_000_000_000,
        }
    }
}

impl Policy {
    /// Smallest quota that can admit a write to a fresh store on Windows.
    pub const MIN_MAX_BYTES: u64 = 458_752;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ranking {
    #[default]
    Bm25,
    Reranked,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Query {
    pub text: String,
    pub boards: Vec<String>,
    pub limit: usize,
    pub ranking: Ranking,
}

/// Validated optional facets for a full-text query.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filters {
    pub kind: Option<String>,
    pub account: Option<String>,
    pub project: Option<String>,
    pub parent_id: Option<String>,
    pub refreshed_after: Option<i64>,
    pub refreshed_before: Option<i64>,
}

/// Where the next page of a board's records starts: after the record with
/// this time and id, in newest-first order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordCursor {
    pub refreshed_at: i64,
    pub id: String,
}

/// A board reading its own retained documents back, newest first.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordQuery {
    pub filters: Filters,
    /// Only records older than this cursor, from a previous page's `next`.
    pub before: Option<RecordCursor>,
    pub limit: usize,
}

/// One page of a board's records, with payloads, and the cursor of the next.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordPage {
    pub documents: Vec<Document>,
    pub next: Option<RecordCursor>,
}

/// One `parent_id` among a board's records: a container such as a
/// conversation, with how many records it holds and its newest time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordGroup {
    pub parent_id: String,
    pub records: u64,
    pub newest: i64,
}

/// A canonical service identity recorded by a board adapter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceIdentity(String);

impl ReferenceIdentity {
    pub fn repository(repository: &str) -> Result<Self, String> {
        canonical("repository", &[repository])
    }

    // qual:api
    pub fn branch(repository: &str, branch: &str) -> Result<Self, String> {
        canonical("branch", &[repository, branch])
    }

    pub fn commit(repository: &str, commit: &str) -> Result<Self, String> {
        canonical("commit", &[repository, commit])
    }

    pub fn github(repository: &str, number: u64) -> Result<Self, String> {
        canonical("github", &[repository, &number.to_string()])
    }

    // qual:api
    pub fn github_copilot_agents(task: &str) -> Result<Self, String> {
        canonical("github-copilot-agents", &[task])
    }

    /// An identity of an app-registered kind; see `register_reference_kind`.
    // qual:api - Apps build identities for their own registered kinds.
    pub fn of_kind(kind: &str, parts: &[&str]) -> Result<Self, String> {
        canonical(kind, parts)
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        let identity = Self(value.to_owned());
        validate_identity(&identity)?;
        Ok(identity)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Why a retained document refers to an identity.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ReferenceMatch {
    Exact,
    Heuristic { evidence: String, confidence: f32 },
}

/// One relation written by a board adapter for a retained document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reference {
    pub identity: ReferenceIdentity,
    pub relation_type: String,
    pub matched: ReferenceMatch,
}

/// Optional index data which is not part of a board's retained payload.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct IndexMetadata {
    pub project: Option<String>,
    pub references: Vec<Reference>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatedQuery {
    pub relation_types: Vec<String>,
    pub boards: Vec<String>,
    pub limit: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RelatedItem {
    pub hit: Hit,
    pub relation_type: String,
    pub matched: ReferenceMatch,
}

fn canonical(kind: &str, parts: &[&str]) -> Result<ReferenceIdentity, String> {
    if parts
        .iter()
        .any(|part| part.trim().is_empty() || part.len() > MAX_REFERENCE_PART_BYTES)
    {
        return Err("Reference identity is incomplete or exceeds its bounds".into());
    }
    let normalized = parts
        .iter()
        .map(|part| part.trim().to_lowercase())
        .collect::<Vec<_>>()
        .join(":");
    ReferenceIdentity::parse(&format!("{kind}:{normalized}"))
}

fn validate_identity(identity: &ReferenceIdentity) -> Result<(), String> {
    let Some((kind, value)) = identity.0.split_once(':') else {
        return Err("Invalid reference identity".into());
    };
    if !crate::reference_kinds::is_known(kind)
        || value.is_empty()
        || identity.0.len() > MAX_REFERENCE_BYTES
    {
        return Err("Invalid reference identity".into());
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchResponse {
    pub hits: Vec<Hit>,
    pub sources: Vec<SourceStatus>,
    pub ranking: String,
    pub warning: Option<String>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceStatus {
    pub board: String,
    pub enabled: bool,
    pub account: Option<String>,
    pub records: u64,
    pub bytes: u64,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RerankerStatus {
    pub installed: bool,
    pub name: String,
    pub download_bytes: u64,
    pub message: String,
}

pub trait Reranker: Send + Sync {
    fn rerank(&self, query: &str, hits: &[Hit]) -> Result<Vec<Hit>, String>;
    fn status(&self) -> RerankerStatus;
    fn install(&self) -> Result<(), String>;
}

/// Capture before a background fetch; use `upsert_leased` on completion.
/// Unlike a scope alone, this fences clear/re-enable and same-account reactivation.
#[derive(Clone, Debug)]
pub struct Lease {
    pub(crate) scope: Scope,
    pub(crate) generation: i64,
    pub(crate) sequence: i64,
}
