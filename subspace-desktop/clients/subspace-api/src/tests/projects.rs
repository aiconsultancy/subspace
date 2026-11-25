use serde_json::{Value, json};
use subspace_framework::client_test_support::Mock;

use crate::{
    Client,
    projects::{Milestone, MilestoneChange, MilestoneFields, MilestoneWriteError, Move},
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
        Err(MilestoneWriteError::Conflict),
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
