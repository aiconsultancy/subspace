use serde_json::{Value, json};
use subspace_framework::client_test_support::Mock;

use crate::{
    Client,
    projects::{
        Milestone, MilestoneChange, MilestoneFields, Move, PlanningWriteError, ProjectFields,
    },
};

fn milestone() -> Milestone {
    serde_json::from_value(json!({
        "id": "milestone-1", "name": "First flight", "status": "next", "version": 7
    }))
    .unwrap()
}

#[test]
fn subspace_milestone_actions_send_the_versioned_contract_once() {
    let mock = Mock::new(|request| {
        let code = match request.method.as_str() {
            "POST" => 201,
            "DELETE" => 204,
            _ => 200,
        };
        (code, "{}".into())
    });
    let api = super::client(&mock);
    let fields = MilestoneFields {
        name: "Flight".into(),
        description: None,
        target_date: None,
    };
    api.add_milestone("project-1", &fields).unwrap();
    api.change_milestone("project-1", &milestone(), MilestoneChange::Edit(fields))
        .unwrap();
    api.change_milestone("project-1", &milestone(), MilestoneChange::Complete(true))
        .unwrap();
    api.change_milestone("project-1", &milestone(), MilestoneChange::Move(Move::Up))
        .unwrap();
    api.change_milestone("project-1", &milestone(), MilestoneChange::Delete)
        .unwrap();
    let requests = mock.requests();
    let actual = requests
        .iter()
        .map(|request| {
            let body: Value = serde_json::from_str(&request.body).unwrap();
            (request.method.as_str(), request.path.as_str(), body)
        })
        .collect::<Vec<_>>();
    let list = "/api/v1/projects/project-1/milestones";
    let item = "/api/v1/projects/project-1/milestones/milestone-1";
    assert_eq!(
        actual,
        [
            (
                "POST",
                list,
                json!({"name":"Flight","description":null,"targetDate":null})
            ),
            ("PATCH", item, json!({"version":7,"name":"Flight"})),
            ("PATCH", item, json!({"version":7,"completed":true})),
            ("PATCH", item, json!({"version":7,"move":"up"})),
            ("DELETE", item, json!({"version":7})),
        ]
    );
}

#[test]
fn nett_milestone_edits_are_scoped_and_a_conflict_is_not_retried() {
    let org = "00000000-0000-4000-8000-000000000001";
    let mock = Mock::new(|_| (409, "{}".into()));
    let api = Client::nett_loopback(&format!("{}/api", mock.base_url()), Some(org), None).unwrap();
    assert_eq!(
        api.change_milestone("project-1", &milestone(), MilestoneChange::Complete(false)),
        Err(PlanningWriteError::Conflict),
    );
    let requests = mock.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "PUT");
    assert_eq!(
        requests[0].path,
        format!("/api/v1/orgs/{org}/projects/project-1/milestones/milestone-1")
    );
    let body: Value = serde_json::from_str(&requests[0].body).unwrap();
    assert_eq!(body, json!({"version":7,"completed":false}));
}

#[test]
fn editing_projects_uses_versioned_put_in_both_backends() {
    let org = "00000000-0000-4000-8000-000000000001";
    let mock = Mock::new(|_| (200, "{}".into()));
    let native = super::client(&mock);
    let nett = Client::nett_loopback(&format!("{}/api", mock.base_url()), Some(org), None).unwrap();
    let fields = ProjectFields {
        version: 4,
        status: Some("paused".into()),
        target_date: Some(None),
        ..Default::default()
    };
    native
        .change_project(
            "project-1",
            &ProjectFields {
                name: Some("Example project".into()),
                version: 4,
                status: Some("paused".into()),
                target_date: Some(None),
                ..Default::default()
            },
        )
        .unwrap();
    nett.change_project("project-1", &fields).unwrap();
    let requests = mock.requests();
    assert_eq!(requests[0].path, "/api/v1/projects/project-1");
    assert_eq!(
        requests[1].path,
        format!("/api/v1/orgs/{org}/projects/project-1")
    );
    assert!(requests.iter().all(|request| request.method == "PUT"));
    assert_eq!(
        serde_json::from_str::<Value>(&requests[0].body).unwrap(),
        json!({"version":4, "name":"Example project", "status":"paused", "targetDate":null}),
    );
    assert_eq!(
        serde_json::from_str::<Value>(&requests[1].body).unwrap(),
        json!({"version":4, "status":"paused", "targetDate":null}),
    );
}

#[test]
fn project_write_reports_conflicts_validation_and_sign_in_without_retries() {
    for (status, expected) in [
        (409, PlanningWriteError::Conflict),
        (422, PlanningWriteError::Invalid),
        (403, PlanningWriteError::Read(crate::Error::SignInNeeded)),
        (401, PlanningWriteError::Read(crate::Error::SignInNeeded)),
    ] {
        let mock = Mock::new(move |_| (status, "{}".into()));
        let fields = ProjectFields {
            version: 4,
            status: Some("started".into()),
            ..Default::default()
        };
        assert_eq!(
            super::client(&mock).change_project("project-1", &fields),
            Err(expected),
        );
        assert_eq!(mock.requests().len(), 1);
    }
}

#[test]
fn milestone_issues_follow_pages_and_keep_the_issue_status() {
    let mock = Mock::new(|request| {
        let mut linked = super::row(if request.path.contains("cursor=next") {
            "2"
        } else {
            "1"
        });
        linked["milestoneId"] = json!("milestone-1");
        if request.path.contains("cursor=next") {
            linked["state"]["name"] = json!("Done");
            linked["state"]["type"] = json!("completed");
            (200, super::page(vec![linked, super::row("3")], None))
        } else {
            (200, super::page(vec![linked], Some("next")))
        }
    });
    let rows = super::client(&mock)
        .milestone_issues("milestone-1")
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].fields.identifier, "SUB-1");
    assert_eq!(rows[1].fields.identifier, "SUB-2");
    assert_eq!(rows[1].fields.state.name, "Done");
    assert!(
        mock.requests()
            .iter()
            .all(|request| request.path.contains("milestone=milestone-1")
                && request.path.contains("includeArchived=true"))
    );
}
