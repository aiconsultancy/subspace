use crate::model::{IssueDetail, LastActivity, Person, Progress, PullRequestSummary, StateSummary};

/// Minimal in-memory tooltip data. Deliberately has no Debug or Serialize.
#[derive(Clone, PartialEq, Eq)]
pub struct ReferenceCard {
    identifier: String,
    title: String,
    summary: Option<String>,
    kind: String,
    state: StateSummary,
    archived: bool,
    assignee: Option<String>,
    facts: ReferenceFacts,
}

impl ReferenceCard {
    pub(in crate::hub) fn from_detail(detail: IssueDetail) -> Self {
        let fields = detail.fields;
        Self {
            identifier: fields.identifier,
            title: fields.title,
            summary: fields.summary,
            kind: fields.kind,
            state: fields.state,
            archived: fields.archived_at.is_some(),
            assignee: fields
                .assignee
                .as_ref()
                .map(|person| person.display_name.clone()),
            facts: ReferenceFacts {
                priority: fields.priority,
                owner: fields.owner,
                assignee: fields.assignee,
                progress: fields.progress,
                pull_requests: fields.pull_request_summary,
                due_date: fields.due_date,
                latest_activity: fields.last_activity,
            },
        }
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn identifier(&self) -> &str {
        &self.identifier
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn title(&self) -> &str {
        &self.title
    }
    // qual:api — the IRC tooltip renders the issue type in the presentation slice.
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn kind(&self) -> &str {
        &self.kind
    }
    // qual:api — the IRC tooltip renders the state in the presentation slice.
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn state(&self) -> &StateSummary {
        &self.state
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn archived(&self) -> bool {
        self.archived
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable reference snapshot boundary."
    pub fn facts(&self) -> &ReferenceFacts {
        &self.facts
    }
    pub fn summary(&self) -> Option<&str> {
        self.summary.as_deref()
    }
    pub fn assignee(&self) -> Option<&str> {
        self.assignee.as_deref()
    }
}

/// A cached outcome by requested identifier. No card means neutral unavailable.
#[derive(Clone, PartialEq, Eq)]
pub struct ResolvedReference {
    identifier: String,
    card: Option<ReferenceCard>,
}

impl ResolvedReference {
    pub(in crate::hub) fn new(identifier: &str, card: Option<&ReferenceCard>) -> Self {
        Self {
            identifier: identifier.into(),
            card: card.cloned(),
        }
    }
    pub fn identifier(&self) -> &str {
        &self.identifier
    }
    pub fn card(&self) -> Option<&ReferenceCard> {
        self.card.as_ref()
    }
}

#[derive(Clone)]
// qual:allow(srp, god_struct) reason: "This immutable contract projection keeps reference cards and their connection scope coherent."
pub struct IrcSnapshot {
    pub(in crate::hub) version: u64,
    pub(in crate::hub) profile_id: String,
    pub(in crate::hub) profile_name: String,
    pub(in crate::hub) api_origin: String,
    pub(in crate::hub) project_keys: Vec<String>,
    pub(in crate::hub) cards: Vec<ResolvedReference>,
    pub(in crate::hub) errors: std::collections::BTreeMap<String, super::ReadError>,
    pub(in crate::hub) account_error: Option<super::ReadError>,
}
impl IrcSnapshot {
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn version(&self) -> u64 {
        self.version
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable contract projection."
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn profile_name(&self) -> &str {
        &self.profile_name
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn api_origin(&self) -> &str {
        &self.api_origin
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn project_keys(&self) -> &[String] {
        &self.project_keys
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn cards(&self) -> &[ResolvedReference] {
        &self.cards
    }
    pub fn reference_error(&self, identifier: &str) -> Option<&super::ReadError> {
        self.account_error
            .as_ref()
            .or_else(|| self.errors.get(identifier))
    }
}

/// Only the optional facts needed by a reference tile; no description, comments or persistence.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReferenceFacts {
    pub priority: i32,
    pub owner: Option<Person>,
    pub assignee: Option<Person>,
    pub progress: Option<Progress>,
    pub pull_requests: Option<PullRequestSummary>,
    pub due_date: Option<String>,
    pub latest_activity: Option<LastActivity>,
}
