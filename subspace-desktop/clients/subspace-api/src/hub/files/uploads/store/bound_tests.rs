//! What bounds the store: uploads under way keep their bytes, only issues
//! holding bytes count against the closed-issue limit, uploads waiting for
//! a check are capped, and a use that finds nothing changed does no work.
use super::store_tests::{ISSUE, add, awaiting_check, close_issues_holding, open, state, upload};
use super::{KEPT_FOR, MAX_UNRESOLVED};
use crate::hub::files::uploads::AttachmentUpload;
use crate::hub::{AttachmentState, State};
use crate::uploads::UploadError;
use std::time::{Duration, Instant};

/// Whether the open issue's uploads can all still be retried.
fn all_retryable(state: &State) -> bool {
    let open = state.open_for_uploads();
    state
        .uploads
        .shown(open.as_ref())
        .all(AttachmentUpload::retryable)
}

#[test]
fn an_upload_under_way_keeps_its_bytes_however_long_or_far_back_its_issue_closed() {
    let mut state = state();
    let start = Instant::now();
    open(&mut state, "OPS-99", Some(ISSUE));
    let busy = upload(8, AttachmentState::Uploading, Some("transfer-busy"), false);
    let id = busy.id.clone();
    add(&mut state, ISSUE, vec![busy]);
    state.snapshot.issue = None;
    state.follow_uploads(start);
    // Eight issues closed since, and its time ran out: its attempt holds
    // the bytes, so letting them go would free nothing.
    close_issues_holding(&mut state, 1..9, start);
    state.follow_uploads(start + KEPT_FOR);
    assert!(!state.uploads.released_transfer("transfer-busy"));
    // The issue opens again, and the attempt fails before sending: Retry
    // can still send it, without the file chosen again.
    open(&mut state, "OPS-99", Some(ISSUE));
    state.follow_uploads(start + KEPT_FOR);
    state.uploads.row(&id).unwrap().state = AttachmentState::Failed(UploadError::Network);
    state.follow_uploads(start + KEPT_FOR);
    assert!(all_retryable(&state));
    // Once no attempt holds them, its bytes go as its issue closes again.
    state.snapshot.issue = None;
    state.follow_uploads(start + KEPT_FOR);
    assert!(
        !state.uploads.contains_transfer("transfer-busy"),
        "it stored nothing"
    );
}

#[test]
fn an_issue_whose_bytes_are_gone_takes_no_place_among_those_keeping_them() {
    let mut state = state();
    let start = Instant::now();
    open(&mut state, "OPS-99", Some(ISSUE));
    add(&mut state, ISSUE, vec![awaiting_check(8, "transfer-gone")]);
    state.snapshot.issue = None;
    state.follow_uploads(start);
    state.follow_uploads(start + KEPT_FOR);
    assert!(state.uploads.released_transfer("transfer-gone"));
    // Eight issues close keeping their bytes, then the first opens and
    // closes again, last of all, with nothing left to hold.
    let later = start + KEPT_FOR;
    close_issues_holding(&mut state, 0..8, later);
    open(&mut state, "OPS-99", Some(ISSUE));
    state.follow_uploads(later + Duration::from_secs(10));
    state.snapshot.issue = None;
    state.follow_uploads(later + Duration::from_secs(11));
    assert!(
        !state.uploads.released_transfer("transfer-0"),
        "only eight closed issues hold bytes"
    );
}

#[test]
fn past_the_most_uploads_waiting_for_a_check_the_oldest_are_forgotten_never_cancelled() {
    let mut state = state();
    let start = Instant::now();
    let issues = MAX_UNRESOLVED as u64 + 1;
    let mut cancels = close_issues_holding(&mut state, 0..issues, start);
    // Every one's time runs out, and each only waits for a check.
    cancels.extend(state.follow_uploads(start + Duration::from_secs(issues) + KEPT_FOR));
    assert!(cancels.is_empty(), "their completion may have landed");
    assert!(!state.uploads.contains_transfer("transfer-0"));
    assert!(state.uploads.contains_transfer("transfer-1"));
    assert!(
        state
            .uploads
            .contains_transfer(&format!("transfer-{MAX_UNRESOLVED}"))
    );
}

#[test]
fn a_use_that_finds_nothing_changed_does_no_work_and_misses_no_change() {
    let mut state = state();
    let start = Instant::now();
    open(&mut state, "OPS-99", Some(ISSUE));
    let waiting = awaiting_check(8, "transfer-waiting");
    let id = waiting.id.clone();
    add(&mut state, ISSUE, vec![waiting]);
    state.snapshot.issue = None;
    state.follow_uploads(start);
    assert!(state.uploads.holds(None, start + KEPT_FOR / 2));
    // Its bytes falling due, another issue opening, or a change to the
    // store each make the next use follow again.
    assert!(!state.uploads.holds(None, start + KEPT_FOR));
    open(&mut state, "OPS-99", Some(ISSUE));
    let reopened = state.open_for_uploads();
    assert!(!state.uploads.holds(reopened.as_ref(), start));
    state.follow_uploads(start);
    assert_eq!(state.uploads.shown(reopened.as_ref()).count(), 1);
    state.uploads.row(&id).unwrap().state = AttachmentState::Uploading;
    assert!(!state.uploads.holds(reopened.as_ref(), start));
}
