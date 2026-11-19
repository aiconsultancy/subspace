//! Per-issue and list activity. System and integration actors can have only a label.
use super::Person;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[non_exhaustive]
pub enum ActivityKind {
    #[serde(rename = "issue.created")]
    IssueCreated,
    #[serde(rename = "issue.imported")]
    IssueImported,
    #[serde(rename = "issue.moved")]
    IssueMoved,
    #[serde(rename = "issue.changed")]
    IssueChanged,
    #[serde(rename = "issue.state-derived")]
    IssueStateDerived,
    #[serde(rename = "issue.archived")]
    IssueArchived,
    #[serde(rename = "issue.unarchived")]
    IssueUnarchived,
    #[serde(rename = "issue.relation-added")]
    IssueRelationAdded,
    #[serde(rename = "issue.relation-removed")]
    IssueRelationRemoved,
    #[serde(rename = "comment.created")]
    CommentCreated,
    #[serde(rename = "comment.imported")]
    CommentImported,
    #[serde(rename = "comment.edited")]
    CommentEdited,
    #[serde(rename = "comment.deleted")]
    CommentDeleted,
    #[serde(rename = "thread.resolved")]
    ThreadResolved,
    #[serde(rename = "thread.unresolved")]
    ThreadUnresolved,
    #[serde(rename = "documents.referenced")]
    DocumentsReferenced,
    #[serde(rename = "documents.unreferenced")]
    DocumentsUnreferenced,
    #[serde(rename = "github.pr-linked")]
    GithubPrLinked,
    #[serde(rename = "github.merge-completed")]
    GithubMergeCompleted,
    #[serde(rename = "decision.logged")]
    DecisionLogged,
    #[serde(rename = "review.submitted")]
    ReviewSubmitted,
    #[serde(rename = "item.created")]
    ItemCreated,
    #[serde(rename = "item.edited")]
    ItemEdited,
    #[serde(rename = "item.completed")]
    ItemCompleted,
    #[serde(rename = "item.reopened")]
    ItemReopened,
    #[serde(rename = "item.removed")]
    ItemRemoved,
    #[serde(rename = "item.moved")]
    ItemMoved,
    #[serde(rename = "list.created")]
    ListCreated,
    #[serde(rename = "list.renamed")]
    ListRenamed,
    #[serde(rename = "list.removed")]
    ListRemoved,
    #[serde(rename = "list.moved")]
    ListMoved,
    #[serde(rename = "list.items-completed")]
    ListItemsCompleted,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum ActivityActorType {
    Human,
    Integration,
    System,
    Agent,
    #[serde(other)]
    Other,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActivityActor {
    #[serde(rename = "type")]
    kind: ActivityActorType,
    user_id: Option<String>,
    person: Option<Person>,
    label: Option<String>,
}

impl ActivityActor {
    pub fn kind(&self) -> ActivityActorType {
        self.kind
    }
    // qual:api - Public actor identity accessor for API consumers.
    pub fn user_id(&self) -> Option<&str> {
        self.user_id.as_deref()
    }
    pub fn person(&self) -> Option<&Person> {
        self.person.as_ref()
    }
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LastActivity {
    kind: ActivityKind,
    occurred_at: String,
    actor: ActivityActor,
    #[serde(rename = "data", default)]
    data: ActivityData,
}

/// Project only bounded known facts, discarding all other activity payloads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ActivityData {
    completed_item_count: Option<u32>,
    decision_id: Option<String>,
}

impl<'de> Deserialize<'de> for ActivityData {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let data = serde_json::Value::deserialize(deserializer)?;
        Ok(Self {
            completed_item_count: data
                .get("completedItemCount")
                .and_then(serde_json::Value::as_u64)
                .and_then(|count| u32::try_from(count).ok()),
            decision_id: data
                .get("decisionId")
                .and_then(serde_json::Value::as_str)
                .filter(|id| uuid::Uuid::parse_str(id).is_ok())
                .map(str::to_owned),
        })
    }
}

impl LastActivity {
    /// Optional count, absent on the minimal list projection.
    pub fn completed_item_count(&self) -> Option<u32> {
        self.data.completed_item_count
    }
    pub fn decision_id(&self) -> Option<&str> {
        self.data.decision_id.as_deref()
    }
    // qual:allow(dry, boilerplate) reason: "DTO readers keep the wire contract immutable."
    pub fn kind(&self) -> ActivityKind {
        self.kind
    }
    // qual:allow(dry, boilerplate) reason: "DTO readers keep the wire contract immutable."
    pub fn occurred_at(&self) -> &str {
        &self.occurred_at
    }
    // qual:allow(dry, boilerplate) reason: "DTO readers keep the wire contract immutable."
    pub fn actor(&self) -> &ActivityActor {
        &self.actor
    }
}

/// A per-issue activity row. The list's LastActivity projection shares the
/// kind, actor and bounded fact decoder with the full timeline.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueActivity {
    pub id: String,
    pub display_at: String,
    pub comment: Option<super::Comment>,
    #[serde(flatten)]
    pub activity: LastActivity,
}

impl IssueActivity {
    pub fn decision_id(&self) -> Option<&str> {
        self.activity.decision_id()
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ActivityPage {
    pub items: Vec<IssueActivity>,
    pub next_cursor: Option<String>,
    #[serde(default, deserialize_with = "optional_references")]
    pub references: ActivityReferences,
}

fn optional_references<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<ActivityReferences, D::Error> {
    Ok(Option::<ActivityReferences>::deserialize(deserializer)?.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    #[test]
    fn optional_references_accepts_null() {
        let mut deserializer = serde_json::Deserializer::from_str("null");
        assert!(
            super::optional_references(&mut deserializer)
                .unwrap()
                .decisions
                .is_empty()
        );
        let mut deserializer = serde_json::Deserializer::from_str("null");
        assert!(
            super::optional_decisions(&mut deserializer)
                .unwrap()
                .is_empty()
        );
    }
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ActivityReferences {
    #[serde(default, deserialize_with = "optional_decisions")]
    pub decisions: Vec<super::DecisionSummary>,
}

fn optional_decisions<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<super::DecisionSummary>, D::Error> {
    Ok(Option::<Vec<super::DecisionSummary>>::deserialize(deserializer)?.unwrap_or_default())
}

impl std::fmt::Debug for ActivityActor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActivityActor")
            .field("kind", &self.kind)
            .field("user_id", &self.user_id)
            .field("text", &"[redacted]")
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for LastActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LastActivity")
            .field("kind", &self.kind)
            .field("occurred_at", &self.occurred_at)
            .field("actor", &self.actor)
            .field("completed_item_count", &self.completed_item_count())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for IssueActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssueActivity")
            .field("id", &self.id)
            .field("display_at", &self.display_at)
            .field("comment", &self.comment)
            .field("activity", &self.activity)
            .finish_non_exhaustive()
    }
}
