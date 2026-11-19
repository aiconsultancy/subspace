//! Copilot agent reads and the assignable-users read, offline.

use std::sync::Arc;

use serde_json::json;
use subspace_core::http::Error;

use super::{client, serve, token};
use crate::copilot_agents::{StartTask, TaskScope};
use crate::model::GhUser;

fn status(error: &Error) -> Option<u16> {
    match error {
        Error::Status { status, .. } => Some(*status),
        _ => None,
    }
}

#[test]
fn copilot_reads_need_a_work_account_before_any_request() {
    let client = client("http://127.0.0.1:9".into(), &token("fixture"));
    let task = StartTask {
        repo: "acme/web".into(),
        prompt: "Fix the build".into(),
        model: None,
        base_ref: None,
        create_pull_request: true,
        subject: None,
    };
    let scope = TaskScope {
        configured_repos: vec!["acme/web".into()],
    };
    let errors = [
        client.copilot_api_url().unwrap_err(),
        client.copilot_agent_session("task-1").unwrap_err(),
        client.list_my_copilot_agent_tasks().unwrap_err(),
        client
            .start_copilot_agent_task_with_submission(&task, &scope, &Default::default())
            .unwrap_err(),
    ];
    for error in errors {
        assert_eq!(status(&error), Some(502), "{error}");
    }
}

#[test]
fn a_resolved_copilot_api_url_is_kept_for_the_run() {
    let client = client("http://127.0.0.1:9".into(), &token("fixture"));
    client
        .copilot_api
        .set("https://copilot.example.test".into())
        .unwrap();
    assert_eq!(
        client.copilot_api_url().unwrap(),
        "https://copilot.example.test"
    );
}

#[test]
fn assignable_users_come_from_the_repository_query() {
    let reply = json!({"data": {"repository": {"assignableUsers": {"nodes": [
        {"login": "ann", "name": "Ann"}, null, {"login": "bob"}
    ]}}}})
    .to_string();
    let url = serve(Arc::new(move |_| (200, reply.clone())));
    let users = client(url, &token("fixture"))
        .assignable("ann", "acme/web")
        .unwrap()
        .users;
    assert_eq!(
        users,
        [
            GhUser {
                login: "ann".into(),
                name: Some("Ann".into())
            },
            GhUser {
                login: "bob".into(),
                name: None
            },
        ]
    );
}
