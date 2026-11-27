mod planning;
mod recovery;
use super::{client, common, detail};
use crate::issue_writes::{ChoiceSource, IssueChanges, IssueProperty, IssueSaveError};
use serde_json::{Value, json};
use subspace_framework::client_test_support::Mock;

fn changes() -> IssueChanges {
    let mut changes = IssueChanges::default();
    changes.set(IssueProperty::Title("A fixture issue".into()));
    changes.set(IssueProperty::Summary(
        "Keep issue context available to the team.".into(),
    ));
    changes
}

#[test]
fn sparse_patch_and_create_use_only_supplied_api_fields() {
    let mock = Mock::new(|r| {
        (
            if r.method == "POST" { 201 } else { 200 },
            detail("1").to_string(),
        )
    });
    let c = client(&mock);
    let mut values = changes();
    values.set(IssueProperty::Description(Some(
        "@[Ada Fixture](member:12345678-1234-1234-1234-123456789abc)".into(),
    )));
    values.set(IssueProperty::Assignee(None));
    values.set(IssueProperty::Estimate(None));
    values.set(IssueProperty::Labels(vec![]));
    values.set(IssueProperty::DueDate(None));
    values.set(IssueProperty::Parent(None));
    values.set(IssueProperty::Priority(0));
    values.set(IssueProperty::Type("issue".into()));
    values.set(IssueProperty::State("state-1".into()));
    c.update_issue("1", 7, &values).unwrap();
    c.create_issue("project-1", &values).unwrap();
    let requests = mock.requests();
    let mut expected = serde_json::to_value(&values).unwrap();
    expected["version"] = json!(7);
    assert_eq!(
        serde_json::from_str::<Value>(&requests[0].body).unwrap(),
        expected
    );
    expected.as_object_mut().unwrap().remove("version");
    expected["projectId"] = json!("project-1");
    assert_eq!(
        serde_json::from_str::<Value>(&requests[1].body).unwrap(),
        expected
    );
    assert_eq!(requests[0].method, "PATCH");
    assert_eq!(requests[0].path, "/api/v1/issues/1");
    assert_eq!(requests[1].path, "/api/v1/issues");
    assert!(requests.iter().all(|r| {
        !r.headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("authorization"))
    }));
}

#[test]
fn errors_never_replay_a_write() {
    for (status, expected) in [
        (400, IssueSaveError::Invalid),
        (401, IssueSaveError::SignIn),
        (403, IssueSaveError::Forbidden),
        (409, IssueSaveError::Conflict),
        (429, IssueSaveError::RateLimited),
        (408, IssueSaveError::Unconfirmed),
        (500, IssueSaveError::Unconfirmed),
    ] {
        let mock = Mock::with_headers(move |_| {
            (
                status,
                vec![("Content-Type".into(), "application/problem+json".into())],
                json!({"currentVersion":8}).to_string(),
            )
        });
        assert_eq!(
            client(&mock).update_issue("1", 1, &changes()).unwrap_err(),
            expected
        );
        assert_eq!(mock.requests().len(), 1);
    }
}

#[test]
fn a_parent_refusal_is_distinct_from_other_create_and_update_conflicts() {
    for (body, create, update) in [
        (
            json!({"errors":{"parentId":["The parent is archived or would create a cycle."]}}),
            IssueSaveError::ParentRefused,
            IssueSaveError::ParentRefused,
        ),
        (
            json!({"currentVersion":8}),
            IssueSaveError::ProjectArchived,
            IssueSaveError::Conflict,
        ),
    ] {
        let mock = Mock::with_headers(move |_| {
            (
                409,
                vec![("Content-Type".into(), "application/problem+json".into())],
                body.to_string(),
            )
        });
        let c = client(&mock);
        assert_eq!(c.create_issue("project-1", &changes()).unwrap_err(), create);
        assert_eq!(c.update_issue("1", 7, &changes()).unwrap_err(), update);
        assert_eq!(mock.requests().len(), 2);
    }
}

#[test]
fn blocked_tracker_children_are_a_validation_error() {
    let mock = Mock::new(|_| {
        (
            400,
            json!({"errors":{"trackerChildren":["A child belongs to an archived project."]}})
                .to_string(),
        )
    });
    assert_eq!(
        client(&mock).update_issue("1", 1, &changes()).unwrap_err(),
        IssueSaveError::TrackerChildrenBlocked,
    );
    assert_eq!(mock.requests().len(), 1);
}

#[test]
fn validation_checks_utf16_dates_and_estimate_and_keeps_null_distinct() {
    let mut values = changes();
    values.set(IssueProperty::Title("🙂".repeat(101)));
    assert_eq!(values.validate(true), Err(IssueSaveError::Title));
    values = changes();
    values.set(IssueProperty::Description(Some("🙂".repeat(25_001))));
    assert_eq!(values.validate(false), Err(IssueSaveError::Description));
    values = changes();
    values.set(IssueProperty::DueDate(Some("2026-02-29".into())));
    assert_eq!(values.validate(false), Err(IssueSaveError::DueDate));
    values = changes();
    values.set(IssueProperty::Estimate(Some(f64::NAN)));
    assert_eq!(values.validate(false), Err(IssueSaveError::Invalid));
    assert_eq!(
        IssueChanges::default().validate(false),
        Err(IssueSaveError::NoChanges)
    );
}

#[test]
fn pickers_page_users_including_agents_states_and_only_active_labels() {
    let mock = Mock::new(|r| {
        if r.path.starts_with("/api/v1/users") {
            let next = !r.path.contains("cursor=");
            return (
                200,
                super::page(
                    vec![
                        json!({"id":if next {"human-1"} else {"agent-1"},"displayName":"Ada Fixture","kind":if next {"human"} else {"agent"}}),
                    ],
                    next.then_some("next"),
                ),
            );
        }
        if r.path.starts_with("/api/v1/labels") {
            assert!(r.path.contains("state=active"));
            return (
                200,
                super::page(
                    vec![
                        json!({"id":"label-1","name":"Fixture","color":"#123456","state":"active"}),
                    ],
                    None,
                ),
            );
        }
        common(r)
    });
    let c = client(&mock);
    let people = c.issue_choices(&ChoiceSource::Assignees).unwrap();
    assert_eq!(people.len(), 2);
    assert!(people[1].name().contains("agent"));
    assert_eq!(
        c.issue_choices(&ChoiceSource::Labels).unwrap()[0].id(),
        "label-1"
    );
    assert_eq!(
        c.issue_choices(&ChoiceSource::States("SUB".into()))
            .unwrap()[0]
            .id(),
        "state-1"
    );
    assert!(!format!("{people:?}").contains("Ada Fixture"));
}

#[test]
fn estimates_follow_the_project_scale_and_missing_scales_offer_no_values() {
    for (scale, expected) in [
        ("fibonacci", vec!["1", "2", "3", "5", "8"]),
        ("exponential", vec!["1", "2", "4", "8", "16"]),
        ("linear", vec!["1", "2", "3", "4", "5"]),
        ("tShirt", vec!["XS", "S", "M", "L", "XL"]),
        ("notUsed", vec![]),
    ] {
        let mock = Mock::new(move |_| (200, json!({"estimateScale":scale}).to_string()));
        let choices = client(&mock)
            .issue_choices(&ChoiceSource::Estimates("SUB".into()))
            .unwrap();
        assert_eq!(
            choices.iter().map(|c| c.name()).collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn create_conflict_explains_archived_project_and_keeps_the_draft() {
    let mock = Mock::new(|_| (409, "{}".into()));
    assert_eq!(
        client(&mock)
            .create_issue("project-1", &changes())
            .unwrap_err(),
        IssueSaveError::ProjectArchived
    );
    assert!(
        IssueSaveError::ProjectArchived
            .to_string()
            .contains("Choose an active project")
    );
    assert_eq!(mock.requests().len(), 1);
}

#[test]
fn the_ready_for_agents_flag_is_sent_only_when_set() {
    let mock = Mock::new(|_| (200, detail("1").to_string()));
    let mut ready = IssueChanges::default();
    ready.set(IssueProperty::ReadyForAgents(true));
    client(&mock).update_issue("1", 3, &ready).unwrap();
    let mut assigned = IssueChanges::default();
    assigned.set(IssueProperty::Assignee(Some("user-7".into())));
    client(&mock).update_issue("1", 4, &assigned).unwrap();
    let bodies: Vec<Value> = mock
        .requests()
        .iter()
        .map(|request| serde_json::from_str(&request.body).unwrap())
        .collect();
    assert_eq!(bodies[0], json!({"readyForAgents": true, "version": 3}));
    assert_eq!(bodies[1], json!({"assigneeId": "user-7", "version": 4}));
}
