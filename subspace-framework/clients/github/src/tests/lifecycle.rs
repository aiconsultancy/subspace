use std::sync::Arc;

use serde_json::json;

use super::{Handler, Request, client, serve, token};
use crate::lifecycle::REFRESH_INTERVAL;

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

#[test]
fn deployment_reads_and_pull_attribution_use_bounded_repository_requests() {
    let handler: Arc<Handler> = Arc::new(|request: &Request| {
        assert_eq!(request.authorization, "Bearer test");
        assert!(request.line.starts_with("GET /repos/owner/repo/"));
        assert!(request.line.ends_with("per_page=100 HTTP/1.1"));
        let response = if request.line.contains("/statuses?") {
            json!([{"id":2,"state":"success"},{"id":1,"state":"in_progress"}])
        } else if request.line.contains("/deployments?") {
            json!([{"id":12,"environment":"dev","sha":SHA}])
        } else {
            json!([{
                "number":7, "merge_commit_sha":SHA, "merged_at":"2026-11-18T16:00:10Z",
                "base":{"repo":{"full_name":"Owner/Repo"}},
                "body":"Closes SUB-7"
            }])
        };
        (200, response.to_string())
    });
    let client = client(serve(handler), &token("test"));
    let pulls = client.recent_closed_pulls("ann", "owner/repo").unwrap();
    assert_eq!(pulls[0].number(), 7);
    assert!(pulls[0].merged_at().is_some());
    assert!(pulls[0].is_merged_into("owner/repo"));
    assert_eq!(pulls[0].body(), Some("Closes SUB-7"));
    assert!(!pulls[0].is_merged_into("owner/other"));

    let deployments = client.recent_deployments("ann", "owner/repo").unwrap();
    let pulls = client
        .commit_pulls("ann", "owner/repo", &deployments[0].sha)
        .unwrap();
    assert_eq!(pulls[0].merge_commit_sha(), Some(SHA));
    assert!(client.commit_pulls("ann", "owner/repo", "main").is_err());
    let statuses = client
        .deployment_history("ann", "owner/repo", deployments[0].id)
        .unwrap();
    assert_eq!(statuses.len(), 2);
    assert_eq!(statuses[0].state, "success");
    assert_eq!(statuses[1].state, "in_progress");
    assert_eq!(REFRESH_INTERVAL.as_secs(), 600);
}
