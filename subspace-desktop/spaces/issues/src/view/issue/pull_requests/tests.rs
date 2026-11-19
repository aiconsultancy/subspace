use super::{heading, state_label};
use serde_json::json;
use subspace_api_client::model::PullRequestSummary;

#[test]
fn states_have_readable_labels_including_unknown_values() {
    for (state, expected) in [
        ("open", "Open"),
        ("merged", "Merged"),
        ("closed", "Closed"),
        ("pending review", "Pending Review"),
        ("queued_for_merge", "Queued For Merge"),
        ("queued-for-merge", "Queued For Merge"),
        ("QUEUED", "Queued"),
        ("", ""),
    ] {
        assert_eq!(state_label(state), expected);
    }
}

#[test]
fn heading_uses_summary_state_counts_and_omits_zeros() {
    let mut summary: PullRequestSummary = serde_json::from_value(json!({
        "total": 99, "open": 2, "merged": 5, "closed": 0,
        "closingPending": 1, "lastObservedAt": null,
    }))
    .unwrap();
    assert_eq!(heading(Some(&summary)), "Pull requests · 2 open · 5 merged");
    summary.closed = 3;
    assert_eq!(
        heading(Some(&summary)),
        "Pull requests · 2 open · 5 merged · 3 closed"
    );
    summary.open = 0;
    summary.merged = 0;
    summary.closed = 0;
    assert_eq!(heading(Some(&summary)), "Pull requests");
    assert_eq!(heading(None), "Pull requests");
}
