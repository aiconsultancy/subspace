use super::ActionsApi as _;
use std::collections::BTreeMap;
mod default_attempt;
mod optimisations;
mod reads;
mod support;
use serde_json::{Value as JsonValue, json};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use subspace_core::http::Error;
use support::{Reply, client, serve};

use super::{
    Definition, DispatchInputType, DispatchRequest, Job, ReviewPendingRequest, ReviewState,
    WorkflowJob, match_job_dependencies, parse_workflow_definition,
};

fn fixture_job(id: u64, name: &str) -> Job {
    serde_json::from_value(json!({"id":id,"name":name,"steps":[]})).unwrap()
}

#[test]
fn workflow_yaml_reads_all_dispatch_types_and_needs_shapes() {
    let yaml = r#"
name: CI
on:
  workflow_dispatch:
    inputs:
      message:
        description: What to say
        type: string
        required: true
        default: hello
      force:
        type: boolean
        default: false
      target:
        type: choice
        options: [dev, prod]
      deployment:
        type: environment
jobs:
  build:
    name: Compile
  test:
    needs: build
  publish:
    needs: [build, test]
"#;
    let Definition::Readable(definition) = parse_workflow_definition(yaml) else {
        panic!("fixture should be readable")
    };
    let dispatch = definition.dispatch.unwrap();
    assert_eq!(dispatch.inputs.len(), 4);
    assert_eq!(dispatch.inputs[0].kind, DispatchInputType::String);
    assert!(dispatch.inputs[0].required);
    assert_eq!(dispatch.inputs[0].default.as_deref(), Some("hello"));
    assert_eq!(dispatch.inputs[1].kind, DispatchInputType::Boolean);
    assert_eq!(dispatch.inputs[1].default.as_deref(), Some("false"));
    assert_eq!(dispatch.inputs[2].kind, DispatchInputType::Choice);
    assert_eq!(dispatch.inputs[2].options, ["dev", "prod"]);
    assert_eq!(dispatch.inputs[3].kind, DispatchInputType::Environment);
    assert_eq!(definition.jobs[1].needs, ["build"]);
    assert_eq!(definition.jobs[2].needs, ["build", "test"]);
}

#[test]
fn workflow_yaml_invalid_and_oversize_are_explicitly_unreadable() {
    let invalid = parse_workflow_definition("jobs: [not-a-map");
    assert!(matches!(invalid, Definition::Unreadable(reason) if reason.contains("invalid")));
    let oversized = "x".repeat(super::definition::MAX_WORKFLOW_BYTES + 1);
    assert!(matches!(
        parse_workflow_definition(&oversized),
        Definition::Unreadable(reason) if reason.contains("1 MB")
    ));
    assert!(matches!(
        parse_workflow_definition("on: workflow_dispatch\njobs: []"),
        Definition::Unreadable(reason) if reason.contains("jobs")
    ));
}

#[test]
fn matrix_jobs_keep_one_api_node_each_and_ambiguity_degrades() {
    let definitions = vec![
        WorkflowJob {
            key: "build".into(),
            name: Some("Build".into()),
            needs: vec![],
        },
        WorkflowJob {
            key: "test".into(),
            name: Some("Test".into()),
            needs: vec!["build".into()],
        },
    ];
    let jobs = vec![
        fixture_job(1, "Build (ubuntu, 20)"),
        fixture_job(2, "Build (windows, 20)"),
        fixture_job(3, "Test"),
    ];
    let Definition::Readable(matched) = match_job_dependencies(&jobs, &definitions) else {
        panic!("matrix fixture should match")
    };
    assert_eq!(matched.len(), 3);
    assert_eq!(matched[0].job_key, "build");
    assert_eq!(matched[1].job_key, "build");
    assert_eq!(matched[2].needs, [1, 2]);

    let ambiguous = vec![
        WorkflowJob {
            key: "a".into(),
            name: Some("Build".into()),
            needs: vec![],
        },
        WorkflowJob {
            key: "b".into(),
            name: Some("Build".into()),
            needs: vec![],
        },
    ];
    assert!(matches!(
        match_job_dependencies(&[fixture_job(4, "Build (ubuntu)")], &ambiguous),
        Definition::Unreadable(reason) if reason.contains("ambiguous")
    ));
}

#[test]
fn list_reads_stop_at_the_page_cap_and_report_truncation() {
    let requests = Arc::new(AtomicUsize::new(0));
    let base_slot = Arc::new(Mutex::new(String::new()));
    let seen = requests.clone();
    let slot = base_slot.clone();
    let base = serve(move |_| {
        let page = seen.fetch_add(1, Ordering::SeqCst) + 1;
        let base = slot.lock().unwrap().clone();
        let mut reply = Reply::json(
            json!({"workflows":[{"id":page,"name":"CI","path":"ci.yml","state":"active"}]})
                .to_string(),
        );
        reply.headers.push((
            "Link".into(),
            format!(
                "<{base}/repos/o/r/actions/workflows?page={}>; rel=\"next\"",
                page + 1
            ),
        ));
        reply
    });
    *base_slot.lock().unwrap() = base.clone();
    let page = client(base).actions_workflows("ann", "o/r").unwrap();
    assert_eq!(page.items.len(), 3);
    assert!(page.truncated);
    assert_eq!(requests.load(Ordering::SeqCst), 3);
}

#[test]
fn large_job_logs_are_bounded_and_marked() {
    let body = "x".repeat(1024 * 1024 + 10);
    let base = serve(move |_| Reply::json(body.clone()));
    let log = client(base).actions_job_log("ann", "o/r", 30).unwrap();
    assert!(log.truncated);
    assert!(
        log.text
            .ends_with("[Subspace truncated this log at 1 MB.]\n")
    );
    assert!(log.text.len() < 1024 * 1024 + 100);
}

#[test]
fn every_actions_write_uses_the_expected_endpoint_and_body() {
    let requests = Arc::new(Mutex::new(Vec::<(String, JsonValue)>::new()));
    let captured = requests.clone();
    let base = serve(move |request| {
        let body = serde_json::from_str(&request.body).unwrap();
        captured.lock().unwrap().push((request.line.clone(), body));
        Reply::status(204, "")
    });
    let client = client(base);
    let inputs = BTreeMap::from([("target".into(), "prod".into())]);
    client
        .dispatch_workflow(
            "ann",
            &DispatchRequest {
                repo: "o/r",
                workflow: "ci.yml",
                reference: "main",
                inputs: &inputs,
            },
        )
        .unwrap();
    client.rerun_workflow("ann", "o/r", 20).unwrap();
    client.rerun_failed_jobs("ann", "o/r", 20).unwrap();
    client.cancel_workflow("ann", "o/r", 20).unwrap();
    client
        .review_pending_deployments(
            "ann",
            &ReviewPendingRequest {
                repo: "o/r",
                run_id: 20,
                environment_ids: &[40],
                state: ReviewState::Approved,
                comment: "ship",
            },
        )
        .unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 5);
    assert!(
        requests[0]
            .0
            .starts_with("POST /repos/o/r/actions/workflows/ci.yml/dispatches ")
    );
    assert_eq!(
        requests[0].1,
        json!({"ref":"main","inputs":{"target":"prod"}})
    );
    assert!(requests[1].0.contains("/actions/runs/20/rerun "));
    assert!(
        requests[2]
            .0
            .contains("/actions/runs/20/rerun-failed-jobs ")
    );
    assert!(requests[3].0.contains("/actions/runs/20/cancel "));
    assert_eq!(
        requests[4].1,
        json!({"environment_ids":[40],"state":"approved","comment":"ship"})
    );
}

#[test]
fn scope_denials_and_rate_limits_keep_actionable_error_mapping() {
    let denied = serve(|_| {
        Reply::status(
            403,
            r#"{"message":"Resource not accessible by personal access token"}"#,
        )
    });
    let error = client(denied)
        .cancel_workflow("ann", "o/r", 20)
        .unwrap_err();
    assert!(
        matches!(error, Error::Status { status: 403, ref message } if message.contains("Actions: write"))
    );

    let limited = serve(|_| {
        let mut reply = Reply::status(403, r#"{"message":"API rate limit exceeded"}"#);
        reply
            .headers
            .push(("X-RateLimit-Remaining".into(), "0".into()));
        reply
            .headers
            .push(("X-RateLimit-Reset".into(), "1790000000".into()));
        reply
    });
    let error = client(limited).actions_workflows("ann", "o/r").unwrap_err();
    assert!(
        matches!(error, Error::Status { status: 429, ref message } if message.contains("resets at"))
    );
}

#[test]
fn rejected_review_requires_a_comment_and_environment() {
    let client = client(serve(|_| Reply::status(500, "{}")));
    assert!(
        client
            .review_pending_deployments(
                "ann",
                &ReviewPendingRequest {
                    repo: "o/r",
                    run_id: 1,
                    environment_ids: &[],
                    state: ReviewState::Approved,
                    comment: "",
                },
            )
            .is_err()
    );
    assert!(
        client
            .review_pending_deployments(
                "ann",
                &ReviewPendingRequest {
                    repo: "o/r",
                    run_id: 1,
                    environment_ids: &[2],
                    state: ReviewState::Rejected,
                    comment: "",
                },
            )
            .is_err()
    );
}
