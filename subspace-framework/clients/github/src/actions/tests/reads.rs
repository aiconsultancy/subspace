use super::{Reply, client, serve};
use crate::actions::{ActionsApi as _, Definition, RunFilters};
use serde_json::json;

fn fixture(path: &str) -> String {
    match path {
        "/repos/o/r/actions/workflows" => json!({"workflows":[{"id":10,"name":"CI","path":".github/workflows/ci.yml","state":"active"}]}).to_string(),
        "/repos/o/r/actions/runs" => json!({"workflow_runs":[{"id":20,"name":"CI","workflow_id":10,"run_number":4,"status":"completed","conclusion":"success"}]}).to_string(),
        "/repos/o/r/actions/runs/20" => json!({"id":20,"name":"CI","status":"completed","head_commit":{"id":"abc","message":"ship"}}).to_string(),
        "/repos/o/r/actions/runs/20/jobs" => json!({"jobs":[{"id":30,"run_id":20,"name":"Build","status":"completed","steps":[{"number":1,"name":"Compile","status":"completed"}]}]}).to_string(),
        "/repos/o/r/actions/jobs/30/logs" => "line one\nline two\n".into(),
        "/repos/o/r/contents/.github/workflows/ci.yml" => "on: workflow_dispatch\njobs:\n  build: {}\n".into(),
        "/repos/o/r/deployments" => json!([{"id":50,"ref":"main","environment":"prod","payload":{}}]).to_string(),
        "/repos/o/r/deployments/50/statuses" => json!([{"id":51,"state":"success"}]).to_string(),
        "/repos/o/r/releases" => json!([{"id":60,"tag_name":"v1","draft":false,"prerelease":false}]).to_string(),
        "/repos/o/r/tags" => json!([{"name":"v1","commit":{"sha":"abc"}}]).to_string(),
        _ => panic!("Unexpected fixture route {path}"),
    }
}

#[test]
fn workflow_run_and_job_reads_map_offline_fixtures() {
    let base = serve(|request| {
        assert_eq!(request.authorization, "Bearer fixture-token");
        let path = request
            .line
            .split_whitespace()
            .nth(1)
            .unwrap()
            .split('?')
            .next()
            .unwrap();
        Reply::json(fixture(path))
    });
    let client = client(base);
    assert_eq!(
        client.actions_workflows("ann", "o/r").unwrap().items[0].id,
        10
    );
    let filters = RunFilters {
        branch: Some("main".into()),
        status: Some("success".into()),
        actor: Some("ann".into()),
        workflow: None,
    };
    assert_eq!(
        client.actions_runs("ann", "o/r", &filters).unwrap().items[0].id,
        20
    );
    assert_eq!(
        client
            .actions_run("ann", "o/r", 20)
            .unwrap()
            .head_commit
            .unwrap()
            .message,
        "ship"
    );
    assert_eq!(
        client.actions_jobs("ann", "o/r", 20).unwrap().items[0]
            .steps
            .len(),
        1
    );
    assert!(
        client
            .actions_job_log("ann", "o/r", 30)
            .unwrap()
            .text
            .contains("line two")
    );
    assert!(matches!(
        client
            .actions_workflow_definition("ann", "o/r", "abc", ".github/workflows/ci.yml")
            .unwrap(),
        Definition::Readable(_)
    ));
}

#[test]
fn deployments_releases_and_tags_map_offline_fixtures() {
    let base = serve(|request| {
        let path = request
            .line
            .split_whitespace()
            .nth(1)
            .unwrap()
            .split('?')
            .next()
            .unwrap();
        Reply::json(fixture(path))
    });
    let client = client(base);
    assert_eq!(
        client
            .actions_deployments("ann", "o/r", Some("prod"))
            .unwrap()
            .items[0]
            .latest_status
            .as_ref()
            .unwrap()
            .state,
        "success"
    );
    assert_eq!(
        client.actions_releases("ann", "o/r").unwrap().items[0].tag_name,
        "v1"
    );
    assert_eq!(
        client.actions_tags("ann", "o/r").unwrap().items[0].name,
        "v1"
    );
}

#[test]
fn pending_reviews_accept_integer_timer_and_user_or_team_reviewers() {
    let base = serve(|request| {
        assert!(
            request
                .line
                .contains("/actions/runs/20/pending_deployments")
        );
        Reply::json(json!([{"environment":{"id":40,"name":"prod"},"wait_timer":30,"wait_timer_started_at":"2026-10-24T08:00:00Z","reviewers":[{"type":"Team","reviewer":{"id":7,"name":"Release team","slug":"release-team"}},{"type":"User","reviewer":{"login":"ann"}}],"current_user_can_approve":true}]).to_string())
    });
    let pending = client(base)
        .actions_pending_deployments("ann", "o/r", 20)
        .unwrap();
    assert!(pending[0].current_user_can_approve);
    assert_eq!(pending[0].wait_timer, 30);
    assert_eq!(
        pending[0].wait_timer_started_at.as_deref(),
        Some("2026-10-24T08:00:00Z")
    );
    let team = pending[0].reviewers[0].reviewer.as_ref().unwrap();
    assert_eq!(team.slug.as_deref(), Some("release-team"));
    assert_eq!(team.login, None);
    assert_eq!(
        pending[0].reviewers[1]
            .reviewer
            .as_ref()
            .unwrap()
            .login
            .as_deref(),
        Some("ann")
    );
}

#[test]
fn environment_policies_are_nested_and_nullable() {
    let base = serve(|request| {
        assert!(request.line.contains("/environments?"));
        Reply::json(json!({"environments":[{"id":40,"name":"prod","deployment_branch_policy":{"protected_branches":true,"custom_branch_policies":false},"protection_rules":[{"id":1,"type":"wait_timer","wait_timer":30},{"id":2,"type":"required_reviewers","reviewers":[{"type":"Team","reviewer":{"name":"Release team","slug":"release-team"}}]}]},{"id":41,"name":"test","deployment_branch_policy":null,"protection_rules":[]}]}).to_string())
    });
    let environments = client(base).actions_environments("ann", "o/r").unwrap();
    let environment = &environments.items[0];
    assert!(
        environment
            .deployment_branch_policy
            .as_ref()
            .unwrap()
            .protected_branches
    );
    assert_eq!(environment.protection_rules[0].wait_timer, Some(30));
    let reviewer = environment.protection_rules[1].reviewers[0]
        .reviewer
        .as_ref()
        .unwrap();
    assert_eq!(reviewer.slug.as_deref(), Some("release-team"));
    assert!(environments.items[1].deployment_branch_policy.is_none());
}
