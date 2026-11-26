use super::{client, pr, serve, token};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use subspace_core::http::Error;

#[test]
fn bound_same_login_with_different_principals_has_distinct_account_ids() {
    let token = token("first");
    let url = serve(Arc::new(move |request| {
        let body: serde_json::Value = serde_json::from_str(&request.body).unwrap();
        assert_eq!(body["query"], "query ViewerBound { viewer { id login } }");
        let id = match request.authorization.as_str() {
            "Bearer first" => "U_first",
            "Bearer second" => "U_second",
            _ => panic!("Unexpected fixture credential"),
        };
        (
            200,
            json!({"data":{"viewer":{"id":id,"login":"ann"}}}).to_string(),
        )
    }));
    let client = client(url, &token);
    let first = client.bind_read("ann").unwrap();
    *token.lock().unwrap() = "second".into();
    let second = client.bind_read("ann").unwrap();
    assert_eq!(first.account(), second.account());
    assert_eq!(first.account_id(), "U_first");
    assert_eq!(second.account_id(), "U_second");
    assert_ne!(first.account_id(), second.account_id());
}

#[test]
fn bound_identity_rejects_missing_or_malformed_node_ids() {
    for viewer in [
        json!({"login":"ann"}),
        json!({"login":"ann","id":null}),
        json!({"login":"ann","id":42}),
        json!({"login":"ann","id":""}),
        json!({"login":"ann","id":"   "}),
        json!({"login":"ann","id":"U_\nsecret"}),
        json!({"login":"ann","id":"U_\u{007f}secret"}),
        json!({"login":"ann","id":"x".repeat(257)}),
    ] {
        let url = serve(Arc::new(move |_| {
            (200, json!({"data":{"viewer":viewer}}).to_string())
        }));
        let error = client(url, &token("captured"))
            .bind_read("ann")
            .err()
            .unwrap();
        assert_eq!(
            error.to_string(),
            "GitHub returned an unexpected viewer identity."
        );
        assert!(!format!("{error:?}").contains("secret"));
    }
}

#[test]
fn bound_id_validation_accepts_limit_without_changing_normal_viewer_query() {
    let url = serve(Arc::new(move |request| {
        let body: serde_json::Value = serde_json::from_str(&request.body).unwrap();
        let viewer = if body["query"] == "query Viewer { viewer { login } }" {
            json!({"login":"ann"})
        } else {
            assert_eq!(body["query"], "query ViewerBound { viewer { id login } }");
            json!({"id":"x".repeat(256),"login":"ann"})
        };
        (200, json!({"data":{"viewer":viewer}}).to_string())
    }));
    let client = client(url, &token("captured"));
    assert_eq!(client.viewer_login(Some("ann")).unwrap(), "ann");
    assert_eq!(client.bind_read("ann").unwrap().account_id().len(), 256);
}

#[test]
fn bound_detail_keeps_verified_token_after_environment_switch() {
    let token = token("captured");
    let swap = token.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let url = serve(Arc::new(move |request| {
        assert_eq!(request.authorization, "Bearer captured");
        assert_eq!(request.line, "POST /graphql HTTP/1.1");
        seen.fetch_add(1, Ordering::SeqCst);
        if request.body.contains("query Viewer") {
            *swap.lock().unwrap() = "other-account-token".into();
            return (
                200,
                json!({"data":{"viewer":{"id":"U_ann","login":"Ann"}}}).to_string(),
            );
        }
        let mut item = pr("P1", 1);
        item["bodyHTML"] = json!("detail text");
        item["commentEvents"] = json!({"nodes":[{"bodyHTML":"comment text","url":"https://untrusted.invalid/comment","author":{"login":"ann"},"createdAt":"2026-10-17"}]});
        (
            200,
            json!({"data":{"repository":{"issueOrPullRequest":item}}}).to_string(),
        )
    }));
    let bound = client(url.clone(), &token).bind_read("ann").unwrap();
    assert_eq!(bound.account(), "ann");
    assert_eq!(bound.account_id(), "U_ann");
    assert_eq!(bound.endpoint(), url);
    let detail = bound.item("o/r#1").unwrap();
    assert_eq!(detail.body_html, "detail text");
    assert!(
        detail
            .events
            .iter()
            .any(|event| event.body_html == "comment text")
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn bound_identity_rejects_wrong_missing_and_forbidden_viewers() {
    for response in [
        json!({"data":{"viewer":{"id":"U_other","login":"other"}}}),
        json!({"data":{"viewer":{"id":"U_ann","login":""}}}),
        json!({"data":{"viewer":null}}),
        json!({"data":{"viewer":{"id":"U_ann","login":"ann"}},"errors":[{"type":"FORBIDDEN","message":"captured"}]}),
    ] {
        let url = serve(Arc::new(move |_| (200, response.to_string())));
        let error = client(url, &token("captured"))
            .bind_read("ann")
            .err()
            .unwrap();
        assert!(!format!("{error:?} {error}").contains("captured"));
    }
}

#[test]
fn bound_rejections_never_retry_or_expose_response_tokens() {
    for status in [400, 401, 403] {
        let token = token("captured");
        let swap = token.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let url = serve(Arc::new(move |request| {
            assert_eq!(request.authorization, "Bearer captured");
            seen.fetch_add(1, Ordering::SeqCst);
            if request.body.contains("query Viewer") {
                return (
                    200,
                    json!({"data":{"viewer":{"id":"U_ann","login":"ann"}}}).to_string(),
                );
            }
            *swap.lock().unwrap() = "fallback".into();
            (status, json!({"message":"captured"}).to_string())
        }));
        let bound = client(url, &token).bind_read("ann").unwrap();
        let error = bound.item("o/r#1").unwrap_err();
        assert!(matches!(error, Error::Status { status: actual, .. } if actual == status));
        assert!(!format!("{error:?} {error}").contains("captured"));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}

#[test]
fn bound_identity_401_is_one_attempt_even_if_environment_changes() {
    let token = token("captured");
    let swap = token.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let url = serve(Arc::new(move |_| {
        seen.fetch_add(1, Ordering::SeqCst);
        *swap.lock().unwrap() = "fallback".into();
        (401, json!({"message":"captured"}).to_string())
    }));
    let error = client(url, &token).bind_read("ann").err().unwrap();
    assert!(matches!(error, Error::Status { status: 401, .. }));
    assert!(!format!("{error:?} {error}").contains("captured"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn bound_identity_never_follows_redirect_to_another_host() {
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::TcpListener,
        thread,
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let other = serve(Arc::new(move |_| {
        seen.fetch_add(1, Ordering::SeqCst);
        (
            200,
            json!({"data":{"viewer":{"id":"U_ann","login":"ann"}}}).to_string(),
        )
    }));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse().unwrap();
            }
        }
        reader.read_exact(&mut vec![0; length]).unwrap();
        write!(stream, "HTTP/1.1 307 Temporary Redirect\r\nLocation: {other}/graphql\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    assert!(client(url, &token("captured")).bind_read("ann").is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn a_new_binding_can_use_the_cli_token_after_github_rejects_the_cached_one() {
    use std::{sync::Mutex, time::Duration};
    use subspace_core::{
        config::Config,
        credentials::{Credentials, Environment, Runner},
    };

    struct Gh(Arc<Mutex<String>>);

    impl Environment for Gh {
        fn var(&self, _: &str) -> Option<String> {
            None
        }
    }

    impl Runner for Gh {
        fn run(&self, _: &str, _: &[&str], _: &[&str], _: Duration) -> Result<String, String> {
            Ok(self.0.lock().unwrap().clone())
        }
    }

    let cli_token = token("old");
    let url = serve(Arc::new(move |request| {
        if request.authorization == "Bearer new" {
            return (
                200,
                json!({"data":{"viewer":{"id":"U_ann","login":"ann"}}}).to_string(),
            );
        }
        (401, r#"{"message":"Bad credentials"}"#.into())
    }));
    let credentials = Credentials::new(
        &Config::default(),
        Gh(cli_token.clone()),
        Gh(cli_token.clone()),
    );
    let client = crate::Client::new(Arc::new(credentials)).with_api_url(url);
    let error = client.bind_read("ann").err().unwrap();
    assert!(matches!(error, Error::Status { status: 401, .. }));

    *cli_token.lock().unwrap() = "new".into();
    assert_eq!(client.bind_read("ann").unwrap().account(), "ann");
}
