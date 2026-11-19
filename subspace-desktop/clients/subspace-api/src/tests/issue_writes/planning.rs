use super::super::{client, detail};
use crate::issue_writes::{ChoiceSource, IssueChanges, IssueProperty};
use serde_json::{Value, json};
use subspace_framework::client_test_support::Mock;

#[test]
fn planning_patch_omits_unchanged_fields_and_distinguishes_clear_from_assignment() {
    let mock = Mock::new(|_| (200, detail("1").to_string()));
    let mut changes = IssueChanges::default();
    changes.set(IssueProperty::Milestone(Some("milestone-1".into())));
    changes.set(IssueProperty::Cycle(None));
    client(&mock).update_issue("1", 9, &changes).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&mock.requests()[0].body).unwrap(),
        json!({"version":9,"milestoneId":"milestone-1","cycleId":null})
    );
    changes.set(IssueProperty::Milestone(None));
    changes.set(IssueProperty::Cycle(Some("cycle-2".into())));
    client(&mock).update_issue("1", 10, &changes).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&mock.requests()[1].body).unwrap(),
        json!({"version":10,"milestoneId":null,"cycleId":"cycle-2"})
    );
}

#[test]
fn planning_catalogues_page_with_project_scope_and_exclude_finished_cycles() {
    let mock = Mock::new(|r| {
        let items = if r.path.contains("milestones") {
            json!([{ "id":"milestone-1", "name":"First delivery" }])
        } else if r.path.contains("cursor=second") {
            json!([{ "id":"cycle-2", "number":2, "state":"upcoming" }])
        } else {
            json!([{ "id":"cycle-1", "number":1, "state":"current" },
                   { "id":"cycle-0", "number":0, "state":"finished" }])
        };
        let cursor =
            (r.path.contains("issue-cycles") && !r.path.contains("cursor=")).then_some("second");
        (200, json!({"items":items,"nextCursor":cursor}).to_string())
    });
    let milestones = client(&mock)
        .issue_choices(&ChoiceSource::Milestones("SUB".into()))
        .unwrap();
    assert_eq!(milestones[0].name(), "First delivery");
    let cycles = client(&mock)
        .issue_choices(&ChoiceSource::Cycles("SUB".into()))
        .unwrap();
    assert_eq!(
        cycles.iter().map(|c| c.id()).collect::<Vec<_>>(),
        ["cycle-1", "cycle-2"]
    );
    assert!(mock.requests().iter().all(|r| r.path.contains("limit=50")));
    assert!(
        mock.requests()
            .iter()
            .filter(|r| r.path.contains("milestones"))
            .all(|r| r.path.contains("includeContent=false"))
    );
    assert!(
        mock.requests()
            .iter()
            .filter(|r| r.path.contains("issue-cycles"))
            .all(|r| r.path.contains("project=SUB"))
    );
}

#[test]
fn assigned_milestone_lookup_filters_by_id_and_does_not_use_an_unrelated_row() {
    let mock = Mock::new(|_| {
        (
            200,
            json!({"items":[{"id":"milestone-1","name":"First delivery"}],"nextCursor":null})
                .to_string(),
        )
    });
    assert_eq!(
        client(&mock)
            .issue_milestone_name("SUB", "milestone-1")
            .unwrap()
            .as_deref(),
        Some("First delivery")
    );
    assert_eq!(
        client(&mock)
            .issue_milestone_name("SUB", "missing")
            .unwrap(),
        None
    );
    assert_eq!(
        mock.requests()[0].path,
        "/api/v1/projects/SUB/milestones?milestoneId=milestone-1&limit=1&includeContent=false"
    );
}

#[test]
fn creation_sends_selected_milestone_and_cycle_ids() {
    let mock = Mock::new(|_| (201, detail("1").to_string()));
    let mut changes = IssueChanges::default();
    changes.set(IssueProperty::Title("Planning fixture".into()));
    changes.set(IssueProperty::Summary(
        "Keep planning choices attached to the issue.".into(),
    ));
    changes.set(IssueProperty::Milestone(Some("milestone-1".into())));
    changes.set(IssueProperty::Cycle(Some("cycle-1".into())));
    client(&mock).create_issue("project-1", &changes).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&mock.requests()[0].body).unwrap(),
        json!({"projectId":"project-1","title":"Planning fixture","summary":"Keep planning choices attached to the issue.","milestoneId":"milestone-1","cycleId":"cycle-1"})
    );
}
