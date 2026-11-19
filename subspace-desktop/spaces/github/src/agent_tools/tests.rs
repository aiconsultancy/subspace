use super::write::{Write, validate_operation, validate_target};
use super::{agent_tools, configured_key, validate_search_repos};
use serde_json::json;
use subspace_framework::agent_tools::test_support;
use subspace_framework::client_github::model::{ItemKind, ReviewEvent};
use subspace_framework::core::config::Config;

mod refresh;

fn config() -> Config {
    let mut config = Config {
        repos: vec!["work/repo".into()],
        ..Default::default()
    };
    config.github.token = Some(subspace_framework::core::Secret::new("test-token"));
    config
}

fn comment(key: &str, body: &str) -> Write {
    Write::Comment {
        key: key.into(),
        body: body.into(),
    }
}

fn update_pr(fields: serde_json::Value) -> Write {
    Write::UpdatePr {
        key: "work/repo#1".into(),
        fields: serde_json::from_value(fields).unwrap(),
    }
}

fn review(event: ReviewEvent) -> Write {
    Write::Review {
        key: "work/repo#1".into(),
        event,
        body: String::new(),
    }
}

fn assignees(key: &str, add: Vec<String>) -> Write {
    Write::Assignees {
        key: key.into(),
        add,
        remove: Vec::new(),
    }
}

#[test]
fn configured_keys_are_case_insensitive_but_not_prefixes() {
    let config = config();
    assert!(configured_key(&config, "WORK/REPO#12").is_ok());
    for key in [
        "work/repo-extra#12",
        "other/repo#12",
        "work/repo#0",
        "work/repo/../other#1",
    ] {
        assert!(configured_key(&config, key).is_err(), "{key}");
    }
}

#[test]
fn every_operation_is_checked_against_the_configured_repositories_and_its_rules() {
    let config = config();
    assert!(validate_operation(&config, &comment("work/repo#1", "first")).is_ok());
    let invalid = [
        comment("other/repo#2", "second"),
        comment("work/repo#2", "   "),
        update_pr(json!({"state":"MERGED"})),
        update_pr(json!({})),
        review(ReviewEvent::RequestChanges),
        assignees("work/repo#1", Vec::new()),
        assignees("work/repo#1", vec!["bad login".into()]),
        assignees("other/repo#1", vec!["octocat".into()]),
    ];
    for operation in invalid {
        assert!(
            validate_operation(&config, &operation).is_err(),
            "{operation:?}"
        );
    }
    assert!(validate_operation(&config, &assignees("work/repo#1", vec!["octocat".into()])).is_ok());
    assert!(validate_operation(&config, &review(ReviewEvent::Approve)).is_ok());
}

#[test]
fn pull_request_operations_need_a_pull_request_but_comments_and_assignees_do_not() {
    let update = update_pr(json!({}));
    assert!(validate_target(&update, ItemKind::Issue, "work/repo#1").is_err());
    assert!(validate_target(&update, ItemKind::Pr, "work/repo#1").is_ok());
    let any = [
        comment("work/repo#1", "x"),
        assignees("work/repo#1", vec!["octocat".into()]),
    ];
    for operation in any {
        assert!(validate_target(&operation, ItemKind::Issue, "work/repo#1").is_ok());
    }
}

#[test]
fn search_defaults_to_fifty_results() {
    let search = subspace_framework::agent_tools::decoded::<super::Read>(
        json!({"action":"github_search","repo":"work/repo","query":"is:pr"}),
    )
    .unwrap();
    assert!(matches!(
        search,
        super::Read::GithubSearch { limit: 50, .. }
    ));
    assert_eq!(super::search_limit(), 50);
}

#[test]
fn search_results_cannot_escape_the_requested_repository() {
    let inside = json!({"search":{"nodes":[{"repository":{"nameWithOwner":"WORK/REPO"}}]}});
    assert!(validate_search_repos(&inside, "work/repo").is_ok());
    let outside = json!({"search":{"nodes":[{"repository":{"nameWithOwner":"outside/repo"}}]}});
    assert!(validate_search_repos(&outside, "work/repo").is_err());
    assert!(validate_search_repos(&json!({"search":{"nodes":[{}]}}), "work/repo").is_err());
}

#[tokio::test]
async fn the_board_advertises_read_and_write_tools_and_scope_has_no_token() {
    let tools = agent_tools(&test_support::environment(config()));
    let names: Vec<&str> = tools.iter().map(|tool| tool.spec().name).collect();
    assert_eq!(names, ["github_read", "github_write"]);
    assert_eq!(
        tools[0].spec().input_schema()["properties"]["action"]["enum"],
        json!([
            "scope",
            "github_search",
            "github_item",
            "github_diff",
            "refresh"
        ])
    );
    assert_eq!(
        tools[1].spec().input_schema()["properties"]["operations"]["items"]["properties"]["action"]
            ["enum"],
        json!([
            "github_comment",
            "github_update_pr",
            "github_review",
            "github_assignees"
        ])
    );
    let (_, invocation) = test_support::invocation(json!({"action":"scope"}), false);
    let scope = tools[0].call(invocation).await.unwrap();
    assert_eq!(scope["repos"], json!(["work/repo"]));
    assert!(!scope.to_string().contains("test-token"));
    let (approval, outside) = test_support::invocation(
        json!({"operations":[{"action":"github_comment","key":"other/repo#1","body":"x"}]}),
        true,
    );
    assert!(tools[1].call(outside).await.is_err());
    assert!(
        approval.summaries().is_empty(),
        "nothing reaches an approval"
    );
}
