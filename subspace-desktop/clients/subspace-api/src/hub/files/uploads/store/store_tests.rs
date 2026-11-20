use super::{Cancel, KEPT_FOR, UploadIssue};
use crate::hub::files::uploads::AttachmentUpload;
use crate::hub::{AttachmentState, IssueSnapshot, IssueStatus, State};
use crate::uploads::UploadError;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(super) const ISSUE: &str = "00000000-0000-4000-8000-0000000000a7";
pub(super) const OTHER: &str = "00000000-0000-4000-8000-0000000000a8";
pub(super) const MIB: usize = 1024 * 1024;

/// An upload of `size` bytes in `state`, holding transfer `transfer` whose
/// bytes went when `sent`.
pub(super) fn upload(
    size: usize,
    state: AttachmentState,
    transfer: Option<&str>,
    sent: bool,
) -> AttachmentUpload {
    let mut upload = crate::hub::files::uploads::admission_tests::upload(8);
    // The bytes held, which may be more than one file's declaration allows.
    upload.bytes = vec![b'a'; size].into();
    upload.id = crate::hub::comments::next_attachment_id();
    upload.state = state;
    upload.transfer = transfer.map(str::to_owned);
    upload.bytes_sent = sent;
    upload
}

/// An upload whose bytes may be stored, waiting for a check.
pub(super) fn awaiting_check(size: usize, transfer: &str) -> AttachmentUpload {
    AttachmentUpload::awaiting_check_for_test(size, transfer)
}

/// Opens the issue `identifier` in `state`, with its detail as `id` once
/// read.
pub(super) fn open(state: &mut State, identifier: &str, id: Option<&str>) {
    let mut issue = IssueSnapshot::new(identifier, crate::PROFILE_ID, IssueStatus::Loaded);
    issue.detail = id.map(|id| serde_json::from_value(crate::tests::detail(id)).unwrap());
    state.snapshot.issue = Some(issue);
}

/// Adds `uploads` to the open issue, read as `id`, as the panel does.
pub(super) fn add(state: &mut State, id: &str, uploads: Vec<AttachmentUpload>) {
    let open = state.snapshot.issue.as_ref().unwrap();
    let issue = UploadIssue {
        id: id.to_owned(),
        identifier: open.identifier.clone(),
    };
    let opening = open.opening;
    for upload in uploads {
        state.attachment_buffers.push(Arc::downgrade(&upload.bytes));
        state.uploads.add(upload, issue.clone(), opening);
    }
}

/// The open issue's uploads, as a snapshot shows them.
pub(super) fn shown(state: &State) -> usize {
    let open = state.open_for_uploads();
    state.uploads.shown(open.as_ref()).count()
}

pub(super) fn state() -> State {
    State::new("http://127.0.0.1:1/api", "NETT")
}

#[test]
fn a_closed_issue_keeps_only_uploads_that_can_still_finish() {
    let mut state = state();
    open(&mut state, "OPS-1", Some(ISSUE));
    let rejected = UploadError::Refused {
        status: 400,
        retry_after: 0,
    };
    let network = AttachmentState::Failed(UploadError::Network);
    add(
        &mut state,
        ISSUE,
        vec![
            upload(8, AttachmentState::Uploading, None, false),
            awaiting_check(8, "transfer-check"),
            upload(8, network, Some("transfer-unsent"), false),
            upload(
                8,
                AttachmentState::Failed(rejected),
                Some("transfer-rejected"),
                true,
            ),
        ],
    );
    state.snapshot.issue = None;
    let cancels = state.follow_uploads(Instant::now());
    // A failure with nothing to check is let go as its issue closes, and its
    // reservation cancelled under the issue's ID.
    assert_eq!(
        cancels,
        [
            (ISSUE.to_owned(), "transfer-unsent".to_owned()),
            (ISSUE.to_owned(), "transfer-rejected".to_owned())
        ]
    );
    open(&mut state, ISSUE, None);
    state.follow_uploads(Instant::now());
    assert_eq!(shown(&state), 2, "under way, and awaiting a check");
}

#[test]
fn a_kept_upload_past_its_time_lets_its_bytes_go_but_never_its_reservation() {
    let mut state = state();
    open(&mut state, "OPS-1", Some(ISSUE));
    let waiting = awaiting_check(1024, "transfer-expired");
    let bytes = Arc::downgrade(&waiting.bytes);
    add(&mut state, ISSUE, vec![waiting]);
    let closed = Instant::now();
    state.snapshot.issue = None;
    assert!(state.follow_uploads(closed).is_empty());
    assert!(state.follow_uploads(closed + KEPT_FOR / 2).is_empty());
    assert_eq!(state.held_bytes(), 1024);
    // Kept long enough: the next use lets its bytes go, but its completion
    // may have landed, so it stays, to be checked, and nothing is cancelled.
    assert!(state.follow_uploads(closed + KEPT_FOR).is_empty());
    assert!(bytes.upgrade().is_none(), "the bytes are released");
    assert_eq!(state.held_bytes(), 0);
    assert!(state.uploads.released_transfer("transfer-expired"));
}

#[test]
fn each_upload_keeps_the_time_its_own_issue_closed() {
    let mut state = state();
    let start = Instant::now();
    open(&mut state, "OPS-1", Some(ISSUE));
    add(&mut state, ISSUE, vec![awaiting_check(8, "transfer-first")]);
    state.snapshot.issue = None;
    state.follow_uploads(start);
    // The issue opens again later, gets another upload, and closes again:
    // both keep their bytes from that later close.
    let later = start + KEPT_FOR / 2;
    open(&mut state, "OPS-1", Some(ISSUE));
    state.follow_uploads(later);
    add(
        &mut state,
        ISSUE,
        vec![awaiting_check(8, "transfer-second")],
    );
    state.snapshot.issue = None;
    state.follow_uploads(later);
    state.follow_uploads(start + KEPT_FOR);
    assert!(!state.uploads.released_transfer("transfer-first"));
    assert!(!state.uploads.released_transfer("transfer-second"));
    state.follow_uploads(later + KEPT_FOR);
    assert!(state.uploads.released_transfer("transfer-first"));
    assert!(state.uploads.released_transfer("transfer-second"));
}

#[test]
fn room_goes_only_as_far_as_lets_the_file_in() {
    let mut state = state();
    let start = Instant::now();
    // Two closed issues keep failed uploads; a third's is still under way.
    for (n, (id, transfer)) in [(OTHER, "transfer-first"), (ISSUE, "transfer-second")]
        .into_iter()
        .enumerate()
    {
        open(&mut state, &format!("OPS-{n}"), Some(id));
        add(&mut state, id, vec![awaiting_check(4 * MIB, transfer)]);
        state.snapshot.issue = None;
        state.follow_uploads(start + Duration::from_secs(n as u64));
    }
    let id = "00000000-0000-4000-8000-0000000000a9";
    open(&mut state, "OPS-9", Some(id));
    let busy = upload(4 * MIB, AttachmentState::Checking, Some("busy"), true);
    add(&mut state, id, vec![busy]);
    state.snapshot.issue = None;
    state.follow_uploads(start + Duration::from_secs(3));
    let held = 60 * MIB;
    // What fits lets nothing go.
    assert_eq!(state.uploads.room(held, 4 * MIB), Ok(()));
    // What would not fit even if every failed one went lets nothing go.
    assert_eq!(state.uploads.room(held, 13 * MIB), Err(UploadError::Budget));
    assert!(!state.uploads.released_transfer("transfer-first"));
    // Otherwise only as many let their bytes go as let it in, the issue
    // closed first first; each stays, to be checked.
    assert_eq!(state.uploads.room(held, 6 * MIB), Ok(()));
    assert!(state.uploads.released_transfer("transfer-first"));
    assert!(!state.uploads.released_transfer("transfer-second"));
    assert!(!state.uploads.released_transfer("busy"));
    assert!(state.uploads.contains_transfer("transfer-first"));
}

#[test]
fn room_counts_only_bytes_letting_go_frees() {
    let mut state = state();
    open(&mut state, "OPS-1", Some(ISSUE));
    let failed = awaiting_check(4 * MIB, "transfer-shared");
    // Something else, an older snapshot or an attempt, still holds its bytes.
    let elsewhere = failed.clone();
    add(&mut state, ISSUE, vec![failed]);
    state.snapshot.issue = None;
    state.follow_uploads(Instant::now());
    let held = 60 * MIB;
    assert_eq!(state.uploads.room(held, 6 * MIB), Err(UploadError::Budget));
    assert!(
        !state.uploads.released_transfer("transfer-shared"),
        "nothing went"
    );
    // Once only the store holds them, letting them go frees them.
    drop(elsewhere);
    assert_eq!(state.uploads.room(held, 6 * MIB), Ok(()));
    assert!(state.uploads.released_transfer("transfer-shared"));
}

#[test]
fn an_upload_kept_past_its_time_comes_back_to_be_checked_as_its_issue_reopens() {
    // Nothing used the store until the issue reopened, by its key or by
    // its ID, just as its time ran out.
    for reference in ["OPS-1", ISSUE] {
        let mut state = state();
        let closed = Instant::now();
        open(&mut state, "OPS-1", Some(ISSUE));
        add(&mut state, ISSUE, vec![awaiting_check(8, "transfer-stale")]);
        state.snapshot.issue = None;
        state.follow_uploads(closed);
        open(&mut state, reference, None);
        let cancels = state.follow_uploads(closed + KEPT_FOR);
        assert!(cancels.is_empty(), "reopened as {reference}");
        assert_eq!(shown(&state), 1, "reopened as {reference}");
        assert!(state.uploads.released_transfer("transfer-stale"));
    }
}

/// Opens, adds an upload waiting for a check, holding `transfer-{n}`, to
/// and closes each issue `n` of `issues`, `n` seconds after `start`, with
/// the reservations the store cancels.
pub(super) fn close_issues_holding(
    state: &mut State,
    issues: std::ops::Range<u64>,
    start: Instant,
) -> Vec<Cancel> {
    let mut cancels = Vec::new();
    for n in issues {
        let id = format!("00000000-0000-4000-8000-{n:012}");
        open(state, &format!("OPS-{n}"), Some(&id));
        add(
            state,
            &id,
            vec![awaiting_check(8, &format!("transfer-{n}"))],
        );
        state.snapshot.issue = None;
        cancels.extend(state.follow_uploads(start + Duration::from_secs(n)));
    }
    cancels
}

#[test]
fn past_eight_closed_issues_the_oldest_lets_its_bytes_go() {
    let mut state = state();
    let cancels = close_issues_holding(&mut state, 0..9, Instant::now());
    assert!(
        cancels.is_empty(),
        "nothing that may be stored is cancelled"
    );
    assert!(state.uploads.released_transfer("transfer-0"));
    assert!(state.uploads.contains_transfer("transfer-0"));
    assert!(!state.uploads.released_transfer("transfer-8"));
}

#[test]
fn kept_uploads_are_found_however_their_issue_opens_again() {
    let mut state = state();
    let now = Instant::now();
    open(&mut state, "OPS-1", Some(ISSUE));
    add(&mut state, ISSUE, vec![awaiting_check(8, "transfer-1")]);
    // Opened again by its ID: shown at once.
    open(&mut state, ISSUE, None);
    state.follow_uploads(now);
    assert_eq!(shown(&state), 1);
    // Opened again under its key, and closed before its detail is read:
    // the upload keeps its issue's ID.
    open(&mut state, "OPS-1", None);
    state.follow_uploads(now);
    assert_eq!(shown(&state), 1);
    state.snapshot.issue = None;
    state.follow_uploads(now);
    // Moved, and opened under its new key: shown once its detail says it
    // is the same issue.
    open(&mut state, "WEB-7", None);
    state.follow_uploads(now);
    assert_eq!(shown(&state), 0, "WEB-7 is not known yet");
    state.snapshot.issue.as_mut().unwrap().detail =
        Some(serde_json::from_value(crate::tests::detail(ISSUE)).unwrap());
    state.follow_uploads(now);
    assert_eq!(shown(&state), 1);
    // Another issue opened under an old key it never had shows nothing.
    open(&mut state, "OPS-1", Some(OTHER));
    state.follow_uploads(now);
    assert_eq!(shown(&state), 0);
}

#[test]
fn uploads_kept_aside_are_never_shown_to_another_account() {
    let mut state = state();
    open(&mut state, "OPS-1", Some(ISSUE));
    add(&mut state, ISSUE, vec![awaiting_check(8, "transfer")]);
    state.snapshot.issue = None;
    state.follow_uploads(Instant::now());
    assert!(state.uploads.contains_transfer("transfer"));
    state.snapshot.me = Some(crate::model::Me {
        user_id: "first".into(),
        kind: "human".into(),
        display_name: "Fixture".into(),
    });
    assert!(state.retire_if_account_changed(&Ok("second".into()), None));
    assert!(!state.uploads.contains_transfer("transfer"));
}
