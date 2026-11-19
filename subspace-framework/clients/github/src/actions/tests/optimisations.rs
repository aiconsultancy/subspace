use super::support::{Reply, client, serve};
use crate::actions::{ActionsApi as _, Definition};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use subspace_core::{
    config::Config,
    credentials::{Credentials, Environment, Runner},
    http::Error,
};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
const YAML: &str = "on: workflow_dispatch\njobs:\n  build: {}\n";
const PATH: &str = ".github/workflows/ci.yml";

#[test]
fn latest_deployment_hydrates_only_one_of_a_hundred_records() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();
    let base = serve(move |request| {
        seen.lock().unwrap().push(request.line.clone());
        if request.line.contains("/statuses?") {
            return Reply::json(r#"[{"id":101,"state":"success"}]"#);
        }
        let records: Vec<_> = (1..=100)
            .map(|id| json!({"id":id,"environment":"prod"}))
            .collect();
        Reply::json(serde_json::to_string(&records).unwrap())
    });
    let latest = client(base)
        .actions_latest_deployment("ann", "o/r", "prod")
        .unwrap()
        .unwrap();
    assert_eq!(latest.id, 1);
    assert_eq!(latest.latest_status.unwrap().state, "success");
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].contains("environment=prod"));
    assert!(requests[0].contains("per_page=1 "));
    assert!(requests[1].contains("/deployments/1/statuses?per_page=1 "));
}

#[test]
fn latest_deployment_empty_history_makes_one_request() {
    let requests = Arc::new(Mutex::new(0));
    let seen = requests.clone();
    let base = serve(move |_| {
        *seen.lock().unwrap() += 1;
        Reply::json("[]")
    });
    assert!(
        client(base)
            .actions_latest_deployment("ann", "o/r", "prod")
            .unwrap()
            .is_none()
    );
    assert_eq!(*requests.lock().unwrap(), 1);
}

#[test]
fn latest_deployment_propagates_list_and_status_errors() {
    for fail_status in [false, true] {
        let requests = Arc::new(Mutex::new(0));
        let seen = requests.clone();
        let base = serve(move |request| {
            *seen.lock().unwrap() += 1;
            if fail_status && !request.line.contains("/statuses?") {
                Reply::json(r#"[{"id":1,"environment":"prod"}]"#)
            } else {
                Reply::status(403, "{}")
            }
        });
        let error = client(base)
            .actions_latest_deployment("ann", "o/r", "prod")
            .unwrap_err();
        assert!(matches!(error, Error::Status { status: 403, .. }));
        assert_eq!(*requests.lock().unwrap(), if fail_status { 2 } else { 1 });
    }
}

#[test]
fn historical_deployments_still_hydrate_every_record() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();
    let base = serve(move |request| {
        seen.lock().unwrap().push(request.line.clone());
        if request.line.contains("/statuses?") {
            Reply::json(r#"[{"id":101,"state":"success"}]"#)
        } else {
            let records: Vec<_> = (1..=100).map(|id| json!({"id":id})).collect();
            Reply::json(serde_json::to_string(&records).unwrap())
        }
    });
    let history = client(base)
        .actions_deployments("ann", "o/r", Some("prod"))
        .unwrap();
    assert_eq!(history.items.len(), 100);
    assert!(
        history
            .items
            .iter()
            .all(|item| item.latest_status.is_some())
    );
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 101);
    assert!(requests[0].contains("per_page=100 "));
}

#[test]
fn immutable_workflows_cache_by_account_repository_commit_and_path() {
    let requests = Arc::new(Mutex::new(0));
    let seen = requests.clone();
    let base = serve(move |_| {
        *seen.lock().unwrap() += 1;
        Reply::json(YAML)
    });
    let client = client(base);
    for (login, repo, sha, path) in [
        ("ann", "o/r", SHA, PATH),
        ("bob", "o/r", SHA, PATH),
        ("ann", "o/other", SHA, PATH),
        (
            "ann",
            "o/r",
            "abcdef0123456789abcdef0123456789abcdef01",
            PATH,
        ),
        ("ann", "o/r", SHA, ".github/workflows/other.yml"),
    ] {
        for _ in 0..2 {
            assert!(matches!(
                client
                    .actions_workflow_definition(login, repo, sha, path)
                    .unwrap(),
                Definition::Readable(_)
            ));
        }
    }
    assert_eq!(*requests.lock().unwrap(), 5);
}

#[test]
fn workflow_branches_tags_and_abbreviated_commits_are_not_cached() {
    let requests = Arc::new(Mutex::new(0));
    let seen = requests.clone();
    let base = serve(move |_| {
        *seen.lock().unwrap() += 1;
        Reply::json(YAML)
    });
    let client = client(base);
    for reference in [
        "main",
        "v1.0",
        "abcdef0",
        "z123456789abcdef0123456789abcdef01234567",
    ] {
        for _ in 0..2 {
            assert!(matches!(
                client
                    .actions_workflow_definition("ann", "o/r", reference, PATH)
                    .unwrap(),
                Definition::Readable(_)
            ));
        }
    }
    assert_eq!(*requests.lock().unwrap(), 8);
}

struct MutableToken(Arc<Mutex<String>>);
impl Environment for MutableToken {
    fn var(&self, name: &str) -> Option<String> {
        (name == "GH_TOKEN").then(|| self.0.lock().unwrap().clone())
    }
}
struct NoCli;
impl Runner for NoCli {
    fn run(&self, _: &str, _: &[&str], _: &[&str], _: Duration) -> Result<String, String> {
        Err("CLI disabled in tests".into())
    }
}

#[test]
fn workflow_cache_tracks_exact_download_credential_and_endpoint() {
    let token = Arc::new(Mutex::new("first-fixture".to_owned()));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();
    let base = serve(move |request| {
        seen.lock().unwrap().push(request.authorization.clone());
        Reply::json(YAML)
    });
    let credentials = Credentials::new(&Config::default(), MutableToken(token.clone()), NoCli);
    let mut client = crate::Client::new(Arc::new(credentials)).with_api_url(base);
    for credential in ["first-fixture", "second-fixture", "first-fixture"] {
        *token.lock().unwrap() = credential.into();
        assert!(matches!(
            client
                .actions_workflow_definition("ann", "o/r", SHA, PATH)
                .unwrap(),
            Definition::Readable(_)
        ));
    }
    assert_eq!(
        *requests.lock().unwrap(),
        ["Bearer first-fixture", "Bearer second-fixture"]
    );
    client = client.with_api_url(serve(|_| Reply::status(403, "{}")));
    assert!(matches!(
        client.actions_workflow_definition("ann", "o/r", SHA, PATH),
        Err(Error::Status { status: 403, .. })
    ));
}

#[test]
fn workflow_retry_caches_only_under_the_credential_that_succeeded() {
    let token = Arc::new(Mutex::new("rejected-fixture".to_owned()));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();
    let rotated = token.clone();
    let base = serve(move |request| {
        seen.lock().unwrap().push(request.authorization.clone());
        if request.authorization == "Bearer rejected-fixture" {
            *rotated.lock().unwrap() = "accepted-fixture".into();
            Reply::status(401, "{}")
        } else {
            Reply::json(YAML)
        }
    });
    let credentials = Credentials::new(&Config::default(), MutableToken(token.clone()), NoCli);
    let client = crate::Client::new(Arc::new(credentials)).with_api_url(base);
    for _ in 0..2 {
        assert!(matches!(
            client
                .actions_workflow_definition("ann", "o/r", SHA, PATH)
                .unwrap(),
            Definition::Readable(_)
        ));
    }
    assert_eq!(
        *requests.lock().unwrap(),
        ["Bearer rejected-fixture", "Bearer accepted-fixture"]
    );
    *token.lock().unwrap() = "rejected-fixture".into();
    assert!(matches!(
        client
            .actions_workflow_definition("ann", "o/r", SHA, PATH)
            .unwrap(),
        Definition::Readable(_)
    ));
    assert_eq!(requests.lock().unwrap().len(), 3);
}

#[test]
fn workflow_cache_preserves_unreadable_and_permission_errors() {
    for (body, status, expected_requests) in [
        ("jobs: [invalid".to_owned(), 200, 1),
        ("x".repeat(1024 * 1024 + 1), 200, 2),
        ("{}".to_owned(), 403, 2),
    ] {
        let requests = Arc::new(Mutex::new(0));
        let seen = requests.clone();
        let base = serve(move |_| {
            *seen.lock().unwrap() += 1;
            Reply::status(status, body.clone())
        });
        let client = client(base);
        for _ in 0..2 {
            let result = client.actions_workflow_definition("ann", "o/r", SHA, PATH);
            if status == 403 {
                assert!(
                    matches!(result, Err(Error::Status { status: 403, message }) if message.contains("Contents: read"))
                );
            } else {
                assert!(matches!(result, Ok(Definition::Unreadable(_))));
            }
        }
        assert_eq!(*requests.lock().unwrap(), expected_requests);
    }
}
