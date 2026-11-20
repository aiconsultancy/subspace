//! An open issue's uploads, kept aside while it is closed: a transfer may
//! finish, or need a check, later. They are kept under the issue's ID, so
//! the issue finds them however it opens again (moved to another team, or
//! by its ID), for [`KEPT_FOR`] and for at most [`MAX_ISSUES`] issues. An
//! upload let go, for that or because it failed with nothing left to check,
//! has its reservation cancelled and its bytes released: bytes out of sight
//! never hold the shared budget for long, and never refuse a new file.
use super::AttachmentUpload;
use crate::hub::{AttachmentState, IssueSnapshot};
use std::time::{Duration, Instant};

/// How long a closed issue's uploads are kept for it to open again.
pub(in crate::hub) const KEPT_FOR: Duration = Duration::from_secs(15 * 60);
/// Closed issues whose uploads are kept at once. Past this, the issue that
/// closed first lets its uploads go.
const MAX_ISSUES: usize = 8;

/// A reservation to cancel: the issue it was made on and its transfer.
pub(in crate::hub) type Cancel = (String, String);

/// One closed issue's uploads.
struct Kept {
    /// The issue's ID, once its detail was read.
    id: Option<String>,
    /// The reference the issue was open under.
    identifier: String,
    uploads: Vec<AttachmentUpload>,
    /// When the issue first closed with them.
    since: Instant,
}

impl Kept {
    /// Whether `reference`, a key or an ID, names this issue.
    fn named(&self, reference: &str) -> bool {
        self.identifier == reference || self.id.as_deref() == Some(reference)
    }

    /// These uploads, split into those `keep` holds and the rest.
    fn split(self, keep: fn(&AttachmentUpload) -> bool) -> (Self, Self) {
        let (kept, rest) = self.uploads.into_iter().partition(keep);
        let issue = |uploads| Self {
            id: self.id.clone(),
            identifier: self.identifier.clone(),
            uploads,
            since: self.since,
        };
        (issue(kept), issue(rest))
    }

    /// Lets every upload go: the reservations to cancel, under the issue's
    /// ID where it is known.
    fn let_go(self) -> Vec<Cancel> {
        let issue = self.id.unwrap_or(self.identifier);
        self.uploads
            .iter()
            .filter_map(|upload| upload.cancel_on(&issue))
            .collect()
    }
}

impl AttachmentUpload {
    /// Whether an attempt is under way: its end settles the row.
    fn in_flight(&self) -> bool {
        !matches!(
            self.state,
            AttachmentState::Failed(_) | AttachmentState::NotFound
        )
    }

    /// Whether a closed issue keeps this upload: an attempt is under way, or
    /// bytes that may be stored wait for a check that can still change the
    /// outcome. A failure with nothing to check is let go.
    pub(in crate::hub) fn worth_keeping(&self) -> bool {
        match self.state {
            AttachmentState::Failed(_) | AttachmentState::NotFound => {
                self.checks() && self.retryable()
            }
            _ => true,
        }
    }

    /// The reservation to cancel when this upload is let go. One under way
    /// cancels its own when its attempt ends and finds no row.
    pub(in crate::hub) fn cancel_on(&self, issue: &str) -> Option<Cancel> {
        self.transfer
            .clone()
            .filter(|_| !self.in_flight())
            .map(|transfer| (issue.to_owned(), transfer))
    }
}

/// Closed issues' uploads, the issue that closed first first.
#[derive(Default)]
pub(in crate::hub) struct Parking {
    issues: Vec<Kept>,
}

impl Parking {
    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.issues.is_empty()
    }

    /// Forgets every upload, without cancelling: for an account or hub that
    /// is gone, whose reservations no request can reach any more.
    pub(in crate::hub) fn clear(&mut self) {
        self.issues.clear();
    }

    /// Keeps the uploads of the issue that closed that are worth keeping and
    /// lets the rest go, and the issue that closed first past
    /// [`MAX_ISSUES`]. Returns the reservations to cancel.
    fn park(&mut self, closed: Kept) -> Vec<Cancel> {
        let (kept, rest) = closed.split(AttachmentUpload::worth_keeping);
        let mut cancels = rest.let_go();
        self.keep(kept);
        cancels.extend(self.overflow());
        cancels
    }

    /// Adds `kept` to what is kept for its issue, or keeps it apart.
    fn keep(&mut self, kept: Kept) {
        if kept.uploads.is_empty() {
            return;
        }
        let same = |other: &Kept| {
            other.named(&kept.identifier) || kept.id.as_deref().is_some_and(|id| other.named(id))
        };
        match self.issues.iter_mut().find(|other| same(other)) {
            Some(other) => {
                other.id = other.id.take().or(kept.id);
                other.uploads.extend(kept.uploads);
            }
            None => self.issues.push(kept),
        }
    }

    /// Lets go of the issues that closed first, past [`MAX_ISSUES`].
    fn overflow(&mut self) -> Vec<Cancel> {
        let excess = self.issues.len().saturating_sub(MAX_ISSUES);
        self.issues.drain(..excess).flat_map(Kept::let_go).collect()
    }

    /// Lets go of the issues closed [`KEPT_FOR`] or longer by `now`.
    fn expire(&mut self, now: Instant) -> Vec<Cancel> {
        let (gone, kept): (Vec<Kept>, Vec<Kept>) = std::mem::take(&mut self.issues)
            .into_iter()
            .partition(|kept| now.saturating_duration_since(kept.since) >= KEPT_FOR);
        self.issues = kept;
        gone.into_iter().flat_map(Kept::let_go).collect()
    }

    /// Lets go of every upload not under way when the shared budget is
    /// `short`: a failed upload out of sight never refuses a new file.
    fn release_idle(&mut self, short: bool) -> Vec<Cancel> {
        if !short {
            return Vec::new();
        }
        let (busy, idle): (Vec<Kept>, Vec<Kept>) = std::mem::take(&mut self.issues)
            .into_iter()
            .map(|kept| kept.split(AttachmentUpload::in_flight))
            .unzip();
        self.issues = busy
            .into_iter()
            .filter(|kept| !kept.uploads.is_empty())
            .collect();
        idle.into_iter().flat_map(Kept::let_go).collect()
    }

    /// Takes the uploads kept for the issue any of `references` names.
    fn take(&mut self, references: &[&str]) -> Vec<AttachmentUpload> {
        let (found, kept): (Vec<Kept>, Vec<Kept>) = std::mem::take(&mut self.issues)
            .into_iter()
            .partition(|kept| references.iter().any(|reference| kept.named(reference)));
        self.issues = kept;
        found.into_iter().flat_map(|kept| kept.uploads).collect()
    }

    fn row(&mut self, id: &str) -> Option<&mut AttachmentUpload> {
        self.issues
            .iter_mut()
            .flat_map(|kept| kept.uploads.iter_mut())
            .find(|upload| upload.id == id)
    }

    /// Takes the upload `id` out, forgetting its issue once it has none.
    fn remove(&mut self, id: &str) -> Option<AttachmentUpload> {
        let (issue, index) = self.issues.iter().enumerate().find_map(|(issue, kept)| {
            let index = kept.uploads.iter().position(|upload| upload.id == id)?;
            Some((issue, index))
        })?;
        let upload = self.issues[issue].uploads.remove(index);
        if self.issues[issue].uploads.is_empty() {
            self.issues.remove(issue);
        }
        Some(upload)
    }
}

impl crate::hub::State {
    /// Keeps the open issue's uploads aside as it closes or another issue
    /// opens in its place, and lets go of uploads kept too long. Returns the
    /// reservations to cancel.
    pub(in crate::hub) fn park_open_uploads(&mut self, now: Instant) -> Vec<Cancel> {
        let closed = self.take_open_uploads(now);
        let mut cancels = self.parked_uploads.expire(now);
        cancels.extend(
            closed
                .into_iter()
                .flat_map(|closed| self.parked_uploads.park(closed)),
        );
        cancels
    }

    /// The open issue's uploads, taken out as it closes at `now`.
    fn take_open_uploads(&mut self, now: Instant) -> Option<Kept> {
        let issue = self.snapshot.issue.as_mut()?;
        Some(Kept {
            id: issue.detail.as_ref().map(|detail| detail.fields.id.clone()),
            identifier: issue.identifier.clone(),
            uploads: std::mem::take(&mut issue.files.uploads),
            since: now,
        })
    }

    /// Shows the uploads kept for `issue` again as it opens, when it opens
    /// under the reference or ID they were kept for.
    pub(in crate::hub) fn unpark_uploads(&mut self, issue: &mut IssueSnapshot) {
        let uploads = self.parked_uploads.take(&[&issue.identifier]);
        issue.files.uploads.extend(uploads);
    }

    /// Shows the uploads kept for the open issue again once its detail says
    /// which issue it is, however it was opened: by another key after a
    /// move, or by its ID.
    pub(in crate::hub) fn unpark_for_detail(&mut self) {
        let Some(issue) = self.snapshot.issue.as_mut() else {
            return;
        };
        let Some(fields) = issue.detail.as_ref().map(|detail| &detail.fields) else {
            return;
        };
        let uploads = self
            .parked_uploads
            .take(&[fields.id.as_str(), fields.identifier.as_str()]);
        issue.files.uploads.extend(uploads);
    }

    /// Whether the upload `id` belongs to the open issue.
    pub(super) fn upload_open(&self, id: &str) -> bool {
        self.snapshot
            .issue
            .as_ref()
            .is_some_and(|open| open.files.uploads.iter().any(|upload| upload.id == id))
    }

    /// The upload `id`, whether its issue is open or its uploads were kept
    /// aside when it closed.
    pub(super) fn upload_row(&mut self, id: &str) -> Option<&mut AttachmentUpload> {
        match self.snapshot.issue.as_mut() {
            Some(open) if open.files.uploads.iter().any(|upload| upload.id == id) => {
                open.files.uploads.iter_mut().find(|upload| upload.id == id)
            }
            _ => self.parked_uploads.row(id),
        }
    }

    /// Takes the upload `id` out, once it is stored or let go.
    pub(super) fn remove_upload(&mut self, id: &str) -> Option<AttachmentUpload> {
        let open = self.snapshot.issue.as_mut().and_then(|open| {
            let index = open
                .files
                .uploads
                .iter()
                .position(|upload| upload.id == id)?;
            Some(open.files.uploads.remove(index))
        });
        open.or_else(|| self.parked_uploads.remove(id))
    }

    /// The bytes drafts and uploads hold across the app once `size` more are
    /// asked for. Uploads kept for closed issues past their time are let go
    /// first, and, if `size` would not fit, those that failed too. Returns
    /// the reservations to cancel.
    pub(in crate::hub) fn room_for(&mut self, size: usize, now: Instant) -> (usize, Vec<Cancel>) {
        let mut cancels = self.parked_uploads.expire(now);
        let short = self.held_bytes().saturating_add(size) > crate::uploads::MAX_TOTAL_BYTES;
        cancels.extend(self.parked_uploads.release_idle(short));
        (self.held_bytes(), cancels)
    }

    /// The bytes still held by drafts and uploads across the app.
    fn held_bytes(&mut self) -> usize {
        self.attachment_buffers
            .retain(|buffer| buffer.strong_count() > 0);
        self.attachment_buffers
            .iter()
            .filter_map(std::sync::Weak::upgrade)
            .map(|buffer| buffer.len())
            .sum()
    }
}

#[cfg(test)]
mod parking_tests {
    use super::KEPT_FOR;
    use crate::hub::{AttachmentState, Hub, IssueSnapshot, IssueStatus, lock};
    use crate::uploads::UploadError;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use subspace_framework::client_test_support::{Mock, Request};

    const UPLOAD: &str = "upload-parked-on-close";
    const ISSUE: &str = "00000000-0000-4000-8000-0000000000a7";

    /// NETT serving attachments: its capability list, the reservation and
    /// the file host. The test answers completion itself.
    fn nett(base: &Arc<Mutex<String>>, request: &Request) -> (u16, String) {
        let path = request.path.as_str();
        if path.ends_with("/desktop/capabilities") {
            (200, r#"{"features":["attachments"]}"#.into())
        } else if path.ends_with("/attachments/prepare") {
            let url = format!("{}/files/{UPLOAD}", lock(base));
            let reply = serde_json::json!({"uploadId": UPLOAD, "uploadUrl": url, "method": "PUT"});
            (200, reply.to_string())
        } else if request.method == "PUT" && path.ends_with(UPLOAD) {
            (200, String::new())
        } else {
            (404, "{}".into())
        }
    }

    fn wait_for(what: &str, done: &dyn Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !done() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// An upload of `size` bytes in `state`, holding transfer `transfer`
    /// whose bytes went when `sent`.
    fn upload(
        size: usize,
        state: AttachmentState,
        transfer: Option<&str>,
        sent: bool,
    ) -> super::AttachmentUpload {
        let mut upload = super::super::admission_tests::upload(8);
        // The bytes held, which may be more than one file's declaration allows.
        upload.bytes = vec![b'a'; size].into();
        upload.id = crate::hub::comments::next_attachment_id();
        upload.state = state;
        upload.transfer = transfer.map(str::to_owned);
        upload.bytes_sent = sent;
        upload
    }

    /// An upload whose bytes may be stored, waiting for a check.
    fn awaiting_check(size: usize, transfer: &str) -> super::AttachmentUpload {
        let unclear = UploadError::Unclear { retry_after: 0 };
        upload(size, AttachmentState::Failed(unclear), Some(transfer), true)
    }

    /// `state`, with the issue `identifier`, read as `id`, open and holding
    /// `uploads`, as once its detail has loaded.
    fn open(
        state: &mut crate::hub::State,
        (identifier, id): (&str, Option<&str>),
        uploads: Vec<super::AttachmentUpload>,
    ) {
        let mut issue = IssueSnapshot::new(identifier, crate::PROFILE_ID, IssueStatus::Loaded);
        issue.detail = id.map(|id| serde_json::from_value(crate::tests::detail(id)).unwrap());
        issue.files.uploads = uploads;
        state.snapshot.issue = Some(issue);
    }

    fn held(state: &mut crate::hub::State, uploads: &[super::AttachmentUpload]) {
        state
            .attachment_buffers
            .extend(uploads.iter().map(|upload| Arc::downgrade(&upload.bytes)));
    }

    #[test]
    fn a_closed_issue_keeps_only_uploads_that_can_still_finish() {
        let mut state = crate::hub::State::new("http://127.0.0.1:1/api", "NETT");
        let rejected = UploadError::Refused {
            status: 400,
            retry_after: 0,
        };
        let uploads = vec![
            upload(8, AttachmentState::Uploading, None, false),
            awaiting_check(8, "transfer-check"),
            upload(
                8,
                AttachmentState::Failed(UploadError::Network),
                Some("transfer-unsent"),
                false,
            ),
            upload(
                8,
                AttachmentState::Failed(rejected),
                Some("transfer-rejected"),
                true,
            ),
        ];
        open(&mut state, ("OPS-1", Some(ISSUE)), uploads);
        let cancels = state.park_open_uploads(Instant::now());
        // A failure with nothing to check is let go and its reservation
        // cancelled, under the issue's ID.
        assert_eq!(
            cancels,
            [
                (ISSUE.to_owned(), "transfer-unsent".to_owned()),
                (ISSUE.to_owned(), "transfer-rejected".to_owned())
            ]
        );
        open(&mut state, (ISSUE, None), Vec::new());
        let mut issue = state.snapshot.issue.take().unwrap();
        state.unpark_uploads(&mut issue);
        assert_eq!(
            issue.files.uploads.len(),
            2,
            "under way, and awaiting a check"
        );
        assert!(state.parked_uploads.is_empty());
    }

    #[test]
    fn kept_uploads_expire_releasing_their_bytes_and_reservations() {
        let mut state = crate::hub::State::new("http://127.0.0.1:1/api", "NETT");
        let waiting = awaiting_check(1024, "transfer-expired");
        let bytes = Arc::downgrade(&waiting.bytes);
        held(&mut state, std::slice::from_ref(&waiting));
        open(&mut state, ("OPS-1", Some(ISSUE)), vec![waiting]);
        let closed = Instant::now();
        assert!(state.park_open_uploads(closed).is_empty());
        assert_eq!(state.room_for(1, closed + KEPT_FOR / 2), (1024, Vec::new()));
        // Once kept long enough, they go, and so do their bytes.
        let (held, cancels) = state.room_for(1, closed + KEPT_FOR);
        assert_eq!(held, 0);
        assert_eq!(cancels, [(ISSUE.to_owned(), "transfer-expired".to_owned())]);
        assert!(bytes.upgrade().is_none(), "the bytes are released");
        assert!(state.parked_uploads.is_empty());
    }

    #[test]
    fn a_failed_upload_out_of_sight_never_refuses_a_new_file() {
        let mib = 1024 * 1024;
        let mut state = crate::hub::State::new("http://127.0.0.1:1/api", "NETT");
        let failed = awaiting_check(40 * mib, "transfer-out-of-sight");
        let busy = upload(
            20 * mib,
            AttachmentState::Checking,
            Some("transfer-busy"),
            true,
        );
        held(&mut state, &[failed.clone(), busy.clone()]);
        open(&mut state, ("OPS-1", Some(ISSUE)), vec![failed, busy]);
        let now = Instant::now();
        assert!(state.park_open_uploads(now).is_empty());
        // What still fits leaves them be.
        assert_eq!(state.room_for(4 * mib, now), (60 * mib, Vec::new()));
        // A file that would not fit lets the failed one go, never one whose
        // attempt is under way.
        let (held, cancels) = state.room_for(8 * mib, now);
        assert_eq!(held, 20 * mib);
        assert_eq!(
            cancels,
            [(ISSUE.to_owned(), "transfer-out-of-sight".to_owned())]
        );
        assert!(!state.parked_uploads.is_empty(), "the busy upload is kept");
    }

    #[test]
    fn at_most_eight_closed_issues_keep_uploads() {
        let mut state = crate::hub::State::new("http://127.0.0.1:1/api", "NETT");
        let now = Instant::now();
        let mut cancels = Vec::new();
        for n in 0..9 {
            let id = format!("00000000-0000-4000-8000-00000000010{n}");
            let transfer = format!("transfer-{n}");
            open(
                &mut state,
                (&format!("OPS-{n}"), Some(&id)),
                vec![awaiting_check(8, &transfer)],
            );
            cancels.extend(state.park_open_uploads(now));
        }
        let first = "00000000-0000-4000-8000-000000000100".to_owned();
        assert_eq!(cancels, [(first, "transfer-0".to_owned())]);
        assert!(state.parked_uploads.take(&["OPS-0"]).is_empty());
        assert_eq!(state.parked_uploads.take(&["OPS-8"]).len(), 1);
    }

    #[test]
    fn kept_uploads_are_found_however_their_issue_opens_again() {
        let mut state = crate::hub::State::new("http://127.0.0.1:1/api", "NETT");
        let now = Instant::now();
        // Opened again by its ID: shown at once.
        open(
            &mut state,
            ("OPS-1", Some(ISSUE)),
            vec![awaiting_check(8, "transfer-1")],
        );
        state.park_open_uploads(now);
        let mut by_id = IssueSnapshot::new(ISSUE, crate::PROFILE_ID, IssueStatus::Loading);
        state.unpark_uploads(&mut by_id);
        assert_eq!(by_id.files.uploads.len(), 1);
        // Moved and opened under its new key: shown once its detail says
        // it is the same issue.
        open(&mut state, ("OPS-1", Some(ISSUE)), by_id.files.uploads);
        state.park_open_uploads(now);
        let mut moved = IssueSnapshot::new("WEB-7", crate::PROFILE_ID, IssueStatus::Loading);
        state.unpark_uploads(&mut moved);
        assert!(moved.files.uploads.is_empty(), "WEB-7 is not known yet");
        state.snapshot.issue = Some(moved);
        state.unpark_for_detail();
        assert!(
            state
                .snapshot
                .issue
                .as_ref()
                .unwrap()
                .files
                .uploads
                .is_empty()
        );
        state.snapshot.issue.as_mut().unwrap().detail =
            Some(serde_json::from_value(crate::tests::detail(ISSUE)).unwrap());
        state.unpark_for_detail();
        assert_eq!(
            state.snapshot.issue.as_ref().unwrap().files.uploads.len(),
            1
        );
        assert!(state.parked_uploads.is_empty());
    }

    /// Whether an upload kept aside for a closed issue has failed.
    fn kept_failure(hub: &Hub) -> bool {
        lock(&hub.0.state)
            .parked_uploads
            .issues
            .iter()
            .flat_map(|kept| &kept.uploads)
            .any(|upload| matches!(upload.state, AttachmentState::Failed(_)))
    }

    #[test]
    fn an_upload_in_progress_when_its_issue_closes_is_shown_again_when_it_reopens() {
        let base = Arc::new(Mutex::new(String::new()));
        let served = Arc::clone(&base);
        let mock = Mock::with_headers(move |request| {
            if request.path.ends_with("/attachments/from-upload") {
                std::thread::sleep(Duration::from_millis(400));
                return (202, vec![("Retry-After".into(), "120".into())], "{}".into());
            }
            let (status, body) = nett(&served, request);
            (status, Vec::new(), body)
        });
        *lock(&base) = mock.base_url().to_owned();
        let client = crate::Client::nett_loopback(
            &format!("{}/api", mock.base_url()),
            Some("00000000-0000-4000-8000-0000000000f1"),
            Some(subspace_framework::core::Secret::new("nett-parked-upload")),
        )
        .unwrap()
        .without_live();
        let hub = Hub::from_client(client, "NETT");
        {
            // The issue is open with its detail, as once it has loaded.
            let mut state = lock(&hub.0.state);
            let mut issue = IssueSnapshot::new("OPS-1", crate::PROFILE_ID, IssueStatus::Loaded);
            issue.detail = Some(serde_json::from_value(crate::tests::detail("1")).unwrap());
            state.snapshot.issue = Some(issue);
        }
        hub.add_attachment(
            "parked.txt",
            "text/plain",
            b"Checked after the issue closed.\n".to_vec(),
        )
        .unwrap();
        // The bytes reached the file host and completion is under way when
        // the person moves away from the issue.
        wait_for("the completion", &|| {
            mock.requests()
                .iter()
                .any(|request| request.path.ends_with("/attachments/from-upload"))
        });
        hub.close_issue();
        wait_for("the outcome, kept aside", &|| kept_failure(&hub));
        hub.open_issue("OPS-1", crate::PROFILE_ID).unwrap();
        let snapshot = hub.snapshot();
        let uploads = snapshot.issue().unwrap().files().uploads();
        assert_eq!(uploads.len(), 1, "the upload is shown again");
        assert!(uploads[0].checks(), "Check completes the same transfer");
        assert!(matches!(
            uploads[0].state(),
            AttachmentState::Failed(UploadError::Unclear { retry_after: 120 })
        ));
        assert!(lock(&hub.0.state).parked_uploads.is_empty());
        hub.stop();
    }

    #[test]
    fn uploads_kept_aside_are_never_shown_to_another_account() {
        let mut state = crate::hub::State::new("http://127.0.0.1:1/api", "NETT");
        open(
            &mut state,
            ("OPS-1", Some(ISSUE)),
            vec![awaiting_check(8, "transfer")],
        );
        state.park_open_uploads(Instant::now());
        assert!(!state.parked_uploads.is_empty());
        state.snapshot.me = Some(crate::model::Me {
            user_id: "first".into(),
            kind: "human".into(),
            display_name: "Fixture".into(),
        });
        assert!(state.retire_if_account_changed(&Ok("second".into()), None));
        assert!(state.parked_uploads.is_empty());
    }
}
