use crate::{CommentIntent, CommentPost, WriteError};
use chrono::{DateTime, Utc};
use std::collections::HashSet;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommentState {
    Editing,
    Posting,
    Failed(WriteError),
    Unclear { refresh: bool },
}

#[derive(Clone)]
pub struct CommentDraft {
    pub(super) body: String,
    pub(super) lease: u64,
    /// The draft NETT keeps this comment's uploaded media under, claimed
    /// by the comment when it is posted. A new one follows each post.
    pub(super) media_draft: String,
    pub(super) images: Vec<super::images::Attachment>,
    pub(super) intakes: usize,
    pub(super) submitted: Option<String>,
    pub(super) state: CommentState,
    pub(super) started: Option<DateTime<Utc>>,
    pub(super) existing_ids: Option<HashSet<String>>,
    pub(super) checking: bool,
    pub(super) notice: Option<&'static str>,
    pub(super) confirmation: u64,
    pub(super) retry_at: Option<Instant>,
    pub(in crate::hub) refresh_failed: bool,
}

impl std::fmt::Debug for CommentDraft {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CommentDraft([redacted])")
    }
}

impl CommentDraft {
    pub(super) fn new() -> Self {
        Self {
            body: String::new(),
            lease: super::images::next_id(),
            media_draft: uuid::Uuid::new_v4().to_string(),
            images: Vec::new(),
            intakes: 0,
            submitted: None,
            state: CommentState::Editing,
            started: None,
            existing_ids: None,
            checking: false,
            notice: None,
            confirmation: 0,
            retry_at: None,
            refresh_failed: false,
        }
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn body(&self) -> &str {
        &self.body
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn state(&self) -> &CommentState {
        &self.state
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn notice(&self) -> Option<&'static str> {
        self.notice
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn confirmation(&self) -> u64 {
        self.confirmation
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn refresh_failed(&self) -> bool {
        self.refresh_failed
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn checking(&self) -> bool {
        self.checking
    }
    pub fn is_frozen(&self) -> bool {
        matches!(
            self.state,
            CommentState::Posting | CommentState::Unclear { .. }
        )
    }
    pub fn is_retry_ready(&self) -> bool {
        !self.checking
            && !matches!(self.state, CommentState::Unclear { refresh: false })
            && self.retry_at.is_none_or(|at| Instant::now() >= at)
    }
    pub fn retry_delay(&self) -> Option<Duration> {
        self.retry_at
            .and_then(|at| at.checked_duration_since(Instant::now()))
    }
    pub fn refusal_reason(&self) -> Option<&'static str> {
        match self.state {
            CommentState::Failed(WriteError::SignInNeeded) => Some(
                "Sign in under Settings › Account, then reload Issues. Your draft is still here.",
            ),
            CommentState::Failed(WriteError::Refused { .. }) => {
                Some("The service refused this comment. Your draft is still here.")
            }
            _ => None,
        }
    }
    pub(super) fn edit(&mut self, text: &str) {
        if !self.is_frozen() {
            self.body = text.into();
            self.state = CommentState::Editing;
            self.notice = None;
        }
    }
    pub(super) fn begin(&mut self, parent: Option<&str>, retry: bool) -> Option<CommentIntent> {
        if !self.attachments_ready()
            || self.state == CommentState::Posting
            || !self.is_retry_ready()
            || (matches!(self.state, CommentState::Unclear { .. }) && !retry)
        {
            return None;
        }
        let intent = match CommentIntent::new(&self.bound_body(), parent) {
            Ok(intent) if self.images.is_empty() => intent,
            Ok(intent) => intent.with_media_draft(&self.media_draft),
            Err(error) => {
                self.state = CommentState::Failed(error);
                return None;
            }
        };
        self.started = Some(Utc::now());
        self.existing_ids = None;
        self.submitted = Some(intent.body().into());
        self.state = CommentState::Posting;
        Some(intent)
    }
    pub(super) fn finish(&mut self, result: &Result<CommentPost, WriteError>) {
        match result {
            Ok(_) => {
                self.body.clear();
                self.images.clear();
                self.media_draft = uuid::Uuid::new_v4().to_string();
                self.submitted = None;
                self.state = CommentState::Editing;
                self.notice = Some("Posted.");
                self.confirmation = self.confirmation.wrapping_add(1);
                self.refresh_failed = false;
                self.retry_at = None;
                self.started = None;
                self.existing_ids = None;
                self.checking = false;
            }
            Err(WriteError::Unclear { retry_after, .. }) => {
                self.state = CommentState::Unclear { refresh: false };
                self.notice =
                    Some("Your comment may have posted. Check discussion before posting again.");
                self.retry_at = Instant::now().checked_add(Duration::from_secs(*retry_after));
            }
            Err(error) => {
                if let WriteError::Refused { retry_after, .. } = error {
                    self.retry_at = Instant::now().checked_add(Duration::from_secs(*retry_after));
                }
                self.state = CommentState::Failed(*error);
            }
        }
    }
}
