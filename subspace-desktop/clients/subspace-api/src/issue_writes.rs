//! Sparse, single-attempt issue writes. The hub owns uncertain-create recovery.
mod changes;
pub(crate) mod choices;
mod summary;
mod transport;
use crate::model::IssueDetail;
pub use changes::{IssueChanges, IssueProperty};
pub use choices::{ChoiceSource, IssueChoice};
use std::sync::atomic::{AtomicU64, Ordering};
pub use summary::{summary_length, valid_summary};

pub const SUMMARY_LIMIT: usize = 300;
const TITLE_LIMIT: usize = 200;
const DESCRIPTION_LIMIT: usize = 50_000;
static NEXT_EDITOR: AtomicU64 = AtomicU64::new(1);

/// Local identity for async results and recovery. Never sent to the service.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct CreateKey(u64);
impl CreateKey {
    pub fn new() -> Self {
        Self(NEXT_EDITOR.fetch_add(1, Ordering::Relaxed))
    }
    pub fn value(&self) -> u64 {
        self.0
    }
}
impl Default for CreateKey {
    fn default() -> Self {
        Self::new()
    }
}

pub enum IssueSave {
    Saved(Box<IssueDetail>),
}
impl std::fmt::Debug for IssueSave {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("IssueSave([redacted])")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum IssueSaveError {
    #[error("Enter a title of 1 to 200 characters on a single line.")]
    Title,
    #[error("Write a description of 50,000 characters or fewer.")]
    Description,
    #[error(
        "Enter a summary of 1 to 300 characters on a single line. A summary cannot be cleared once set."
    )]
    Summary,
    #[error("Enter a due date in YYYY-MM-DD format.")]
    DueDate,
    #[error("There's nothing to save.")]
    NoChanges,
    #[error("Subspace API couldn't save these values. Check the fields and try again.")]
    Invalid,
    #[error("This issue or something it links to changed. Read the latest issue before saving.")]
    Conflict,
    #[error(
        "This project is archived. Choose an active project to create the issue. Your draft is still here."
    )]
    ProjectArchived,
    #[error("This issue is archived. Restore it before saving. Your draft is still here.")]
    IssueArchived,
    #[error("Several new issues match this draft. Check the issue list before creating another.")]
    SeveralMatches,
    #[error("Sign in under Settings › Account, then reload Issues. Your draft is still here.")]
    SignIn,
    #[error("You don't have permission to save this issue. Your draft is still here.")]
    Forbidden,
    #[error("This project or issue is no longer available. Your draft is still here.")]
    Unavailable,
    #[error("Couldn't prepare the save. Your draft is still here; try again when connected.")]
    NotSent,
    #[error("Couldn't confirm the save. Check the issue before trying again.")]
    Unconfirmed,
    #[error("The service is busy. Wait before saving again. Your draft is still here.")]
    RateLimited,
    #[error(
        "An archived sub-issue can't move with its parent. Restore it, then refresh the preview."
    )]
    ArchivedSubIssue,
}

pub(crate) enum SaveTarget<'a> {
    Create { project: &'a str },
    Update { issue: &'a str, version: u32 },
}
