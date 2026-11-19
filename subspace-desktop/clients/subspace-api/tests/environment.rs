//! Isolate environment overrides in a subprocess; parallel tests never mutate env.
use std::process::Command;
use subspace_api_client::{Client, CommentIntent, configure_api_origin};
use subspace_framework::client_test_support::{Mock, Request};

const BUILD_ORIGIN: &str = "https://api.fixture.example.test/api";

#[test]
fn environment_overrides_are_debug_only_and_loopback_bypasses_proxies() {
    let api = Mock::new(environment_response);
    let proxy = Mock::new(|_| (502, "unexpected proxy request".into()));
    let base = format!("{}/api", api.base_url());
    let path = std::env::temp_dir().join(format!("subspace-env-{}-assertion", std::process::id()));
    std::fs::write(&path, "fixture-only\n").unwrap();
    let expected = if cfg!(debug_assertions) {
        base.as_str()
    } else {
        BUILD_ORIGIN
    };
    let mut child = Command::new(std::env::current_exe().unwrap());
    child
        .args(["--ignored", "--exact", "client_environment_probe"])
        .env("SUBSPACE_API_URL", &base)
        .env("SUBSPACE_API_ACCESS_JWT_FILE", &path)
        .env("SUBSPACE_EXPECTED_ORIGIN", expected)
        .env_remove("NO_PROXY")
        .env_remove("no_proxy");
    for name in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ] {
        child.env(name, proxy.base_url());
    }
    let result = child.output().unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(
        result.status.success(),
        "{} {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(proxy.requests().is_empty());
    let requests = api.requests();
    if cfg!(debug_assertions) {
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|request| {
            request
                .headers
                .contains(&("authorization".into(), "Bearer fixture-only".into()))
        }));
    } else {
        assert!(requests.is_empty());
    }
}

#[test]
fn ordinary_build_uses_the_configured_origin_without_a_fixture_override() {
    let result = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "client_environment_probe"])
        .env_remove("SUBSPACE_API_URL")
        .env("SUBSPACE_EXPECTED_ORIGIN", BUILD_ORIGIN)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
}

fn environment_response(request: &Request) -> (u16, String) {
    if request.method == "POST" {
        return (201, serde_json::json!({"id":"saved","issueId":"1","parentCommentId":null,
            "body":"Fixture text","version":1,"actualAuthor":{"userId":"me","kind":"human","displayName":"Ada Fixture","isFormerMember":false},
            "createdAt":"2026-11-16T12:00:00Z","displayCreatedAt":"2026-11-16T12:00:00Z",
            "resolved":false,"replyCount":0,"hasReplies":false}).to_string());
    }
    (
        200,
        r#"{"userId":"me","kind":"human","displayName":"Ada Fixture"}"#.into(),
    )
}

#[test]
#[ignore = "subprocess fixture, invoked by environment_overrides_are_debug_only_and_loopback_bypasses_proxies"]
fn client_environment_probe() {
    configure_api_origin(BUILD_ORIGIN).unwrap();
    let client = Client::new().unwrap();
    assert_eq!(
        client.api_origin(),
        std::env::var("SUBSPACE_EXPECTED_ORIGIN").unwrap()
    );
    if client.api_origin() != BUILD_ORIGIN {
        assert_eq!(client.me().unwrap().user_id, "me");
        assert!(
            client
                .post_comment("SUB-1", &CommentIntent::new("Fixture text", None).unwrap())
                .is_ok()
        );
    }
}
