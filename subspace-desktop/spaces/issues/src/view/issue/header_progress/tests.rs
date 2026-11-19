use super::{completion, counts, percent, progress};
use serde_json::json;
use subspace_api_client::model::IssueDetail;

fn detail(done: i32, total: i32) -> IssueDetail {
    let mut value = crate::tests::offline::api("/api/v1/issues/SUB-1", &Default::default());
    value["progress"] = json!({"done": done, "total": total, "canceled": 0,
        "duplicate": 0, "sample": [], "hasMore": false});
    serde_json::from_value(value).unwrap()
}

#[test]
fn progress_requires_a_tracker_with_a_positive_total() {
    for total in [0, -1] {
        assert!(progress(&detail(0, total)).is_none());
    }
    let mut issue = detail(9, 25);
    assert!(progress(&issue).is_some());
    issue.fields.kind = "issue".into();
    assert!(progress(&issue).is_none());
    issue.fields.progress = None;
    assert!(progress(&issue).is_none());
}

#[test]
fn progress_text_and_figure_clamp_the_same_completed_count() {
    for (done, expected, text) in [
        (9, 9, "9 of 25 done"),
        (30, 25, "25 of 25 done"),
        (-1, 0, "0 of 25 done"),
    ] {
        let detail = detail(done, 25);
        let p = progress(&detail).unwrap();
        assert_eq!(counts(p).0, expected);
        assert_eq!(completion(p), text);
        assert_eq!(percent(counts(p).0, p.total), expected as f32 * 4.);
    }
}

#[test]
fn marks_stop_at_sixteen_and_exclusions_require_nonzero_counts() {
    for (total, strip) in [(16, true), (17, false)] {
        let mut detail = detail(9, total);
        let p = detail.fields.progress.as_mut().unwrap();
        assert_eq!(counts(p), (9, strip, false));
        p.canceled = 2;
        assert_eq!(counts(p), (9, strip, true));
        p.canceled = 0;
        p.duplicate = 1;
        assert_eq!(counts(p), (9, strip, true));
    }
}
