use serde::Deserialize;

use super::{IssueMedia, MentionStatus, Person, RelayedCrew};

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Comment {
    pub id: String,
    pub issue_id: String,
    pub parent_comment_id: Option<String>,
    pub body: String,
    pub version: i64,
    pub actual_author: Person,
    pub created_at: String,
    pub display_created_at: String,
    pub edited_at: Option<String>,
    pub deleted_at: Option<String>,
    pub resolved: bool,
    pub resolved_by: Option<Person>,
    pub resolved_at: Option<String>,
    pub reply_count: i32,
    pub has_replies: bool,
    pub provenance: Option<CommentProvenance>,
    pub relayed_crew: Option<RelayedCrew>,
    #[serde(default)]
    pub media: Vec<IssueMedia>,
    #[serde(default)]
    pub mentions: Vec<MentionStatus>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct CommentProvenance {
    pub source_system: String,
    pub original_url: Option<String>,
    pub original_author_display_name: Option<String>,
    pub original_created_at: Option<String>,
}

impl std::fmt::Debug for Comment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Comment")
            .field("id", &self.id)
            .field("issue_id", &self.issue_id)
            .field("parent_comment_id", &self.parent_comment_id)
            .field("body", &"[redacted]")
            .field("author_id", &self.actual_author.user_id)
            .field("reply_count", &self.reply_count)
            .field("media_count", &self.media.len())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for CommentProvenance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommentProvenance")
            .field("text", &"[redacted]")
            .finish_non_exhaustive()
    }
}
