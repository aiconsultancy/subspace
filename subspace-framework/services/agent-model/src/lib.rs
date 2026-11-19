//! Records shared by chat runtimes and reusable dashboard UI. No runtime or UI dependencies.
mod image_attachment;
pub use image_attachment::{DRAFT_ATTACHMENT_LIMIT, ImageAttachment};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextRef {
    pub key: String,
    pub label: String,
    pub content: String,
}

/// A saved conversation of the current account.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionSummary {
    pub id: String,
    /// The session name, else a title cleaned from its summary.
    pub title: String,
    /// How long ago the session changed, ready to show, such as "5 min ago".
    pub modified: String,
    /// ISO 8601 modification time, as the SDK reported it, for grouping.
    pub modified_at: String,
    /// ISO 8601 creation time, as the SDK reported it.
    pub created_at: String,
}
