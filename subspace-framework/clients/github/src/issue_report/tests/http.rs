use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::super::{ErrorKind, Outcome, TransportKind, prepare};
use super::{Draft, diagnostics, repository};
use crate::tests::{client, serve, token};

mod fixture;

const CREATED: &str =
    "{\"number\":42,\"html_url\":\"https://github.com/example-org/example-app/issues/42\"}";

#[test]
fn authenticated_creation_returns_verified_link_and_skips_missing_labels() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let base = serve(Arc::new(move |_| {
        if seen.fetch_add(1, Ordering::SeqCst) == 0 {
            (201, CREATED.into())
        } else {
            (404, "{\"message\":\"not found\"}".into())
        }
    }));
    let client = client(base, &token("test-only-nonworking-credential"));
    let report = prepare(&Draft::new("description"), &diagnostics(), &repository()).unwrap();
    let plan = client.prepare_issue_report(report);
    assert_eq!(plan.transport(), TransportKind::Authenticated);
    assert_eq!(plan.body(), plan.report().body());
    let Outcome::Created(created) = client.submit_issue_report(plan).unwrap() else {
        panic!("expected API creation")
    };
    assert_eq!(created.number(), 42);
    assert_eq!(
        created.url(),
        "https://github.com/example-org/example-app/issues/42"
    );
    assert!(created.applied_labels().is_empty());
    assert_eq!(created.skipped_labels(), ["unknown", "source:subspace"]);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[test]
fn failures_are_friendly_and_issue_creation_is_never_retried() {
    for (status, body, expected) in [
        (
            401,
            "{\"message\":\"TEST_ONLY_PRIVATE_API_TEXT\"}",
            ErrorKind::Authentication,
        ),
        (
            403,
            "{\"message\":\"TEST_ONLY_PRIVATE_API_TEXT\"}",
            ErrorKind::AccessDenied,
        ),
        (404, "{}", ErrorKind::RepositoryUnavailable),
        (422, "{}", ErrorKind::Rejected),
        (429, "{}", ErrorKind::RateLimited),
        (500, "{}", ErrorKind::UncertainCreation),
        (
            201,
            "malformed accepted response",
            ErrorKind::UncertainCreation,
        ),
        (
            201,
            "{\"number\":42,\"html_url\":\"https://evil.invalid/42\"}",
            ErrorKind::UncertainCreation,
        ),
    ] {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let base = serve(Arc::new(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
            (status, body.into())
        }));
        let client = client(base, &token("test-only-nonworking-credential"));
        let plan = client.prepare_issue_report(
            prepare(&Draft::new("description"), &diagnostics(), &repository()).unwrap(),
        );
        let error = client.submit_issue_report(plan).unwrap_err();
        assert_eq!(error.kind(), expected);
        assert!(!format!("{error:?} {error}").contains("TEST_ONLY_PRIVATE_API_TEXT"));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn creation_sends_exact_preview_without_labels_then_applies_only_existing_labels() {
    let (base, requests) = fixture::capture(vec![
        (201, CREATED.into()),
        (200, "{\"name\":\"unknown\"}".into()),
        (404, "{}".into()),
        (200, "[{\"name\":\"unknown\"}]".into()),
    ]);
    let client = client(base, &token("test-only-nonworking-credential"));
    let report = prepare(
        &Draft::new("exact\n description  ")
            .with_title("Specific title")
            .with_steps("1. exact steps\n"),
        &diagnostics(),
        &repository(),
    )
    .unwrap();
    let expected_body = report.body().to_owned();
    let Outcome::Created(created) = client
        .submit_issue_report(client.prepare_issue_report(report))
        .unwrap()
    else {
        panic!("expected API creation")
    };
    assert_eq!(created.applied_labels(), ["unknown"]);
    assert_eq!(created.skipped_labels(), ["source:subspace"]);
    let captured: Vec<_> = requests.try_iter().collect();
    assert_eq!(captured.len(), 4);
    assert_eq!(
        captured[0].line,
        "POST /repos/example-org/example-app/issues HTTP/1.1"
    );
    assert_eq!(
        captured[0].body,
        serde_json::json!({"title":"Specific title", "body":expected_body})
    );
    assert_eq!(
        captured[0].authorization,
        "Bearer test-only-nonworking-credential"
    );
    assert_eq!(
        captured[1].line,
        "GET /repos/example-org/example-app/labels/unknown HTTP/1.1"
    );
    assert_eq!(
        captured[2].line,
        "GET /repos/example-org/example-app/labels/source%3Asubspace HTTP/1.1"
    );
    assert_eq!(
        captured[3].line,
        "POST /repos/example-org/example-app/issues/42/labels HTTP/1.1"
    );
    assert_eq!(captured[3].body, serde_json::json!({"labels":["unknown"]}));
}

#[test]
fn forbidden_or_unparseable_label_update_preserves_created_issue() {
    for (status, body) in [(403, "{}"), (200, "invalid JSON")] {
        let (base, requests) = fixture::capture(vec![
            (201, CREATED.into()),
            (200, "{\"name\":\"unknown\"}".into()),
            (200, "{\"name\":\"source:subspace\"}".into()),
            (status, body.into()),
        ]);
        let client = client(base, &token("test-only-nonworking-credential"));
        let plan = client.prepare_issue_report(
            prepare(&Draft::new("description"), &diagnostics(), &repository()).unwrap(),
        );
        let Outcome::Created(created) = client.submit_issue_report(plan).unwrap() else {
            panic!("created issue was lost")
        };
        assert_eq!(created.number(), 42);
        assert!(created.applied_labels().is_empty());
        assert_eq!(created.skipped_labels().len(), 2);
        assert_eq!(requests.try_iter().count(), 4);
    }
}

#[test]
fn missing_auth_uses_reviewed_browser_plan_without_http_or_cli_writes() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let base = serve(Arc::new(move |_| {
        seen.fetch_add(1, Ordering::SeqCst);
        (500, "{}".into())
    }));
    let credentials = subspace_core::credentials::Credentials::new(
        &subspace_core::config::Config::default(),
        fixture::NoAuth,
        fixture::NoCli,
    );
    let client = crate::Client::new(Arc::new(credentials)).with_api_url(base);
    let report = prepare(
        &Draft::new("界".repeat(840)).with_title("a"),
        &diagnostics(),
        &repository(),
    )
    .unwrap();
    let plan = client.prepare_issue_report(report);
    assert_eq!(plan.transport(), TransportKind::Browser);
    assert_eq!(plan.body(), plan.browser_fallback().body());
    let expected = plan.body().to_owned();
    let Outcome::Browser(fallback) = client.submit_issue_report(plan).unwrap() else {
        panic!("missing auth must not create")
    };
    assert_eq!(fallback.body(), expected);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn accepted_post_with_lost_connection_is_uncertain_not_retried() {
    let (base, requests) = fixture::capture(vec![(0, String::new())]);
    let client = client(base, &token("test-only-nonworking-credential"));
    let plan = client.prepare_issue_report(
        prepare(&Draft::new("description"), &diagnostics(), &repository()).unwrap(),
    );
    let error = client.submit_issue_report(plan).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::UncertainCreation);
    assert!(error.to_string().contains("before retrying"));
    assert_eq!(requests.try_iter().count(), 1);
}

#[test]
fn gh_active_account_is_used_without_switching_global_identity() {
    let (base, requests) = fixture::capture(vec![
        (201, CREATED.into()),
        (403, "{}".into()),
        (403, "{}".into()),
    ]);
    let home = fixture::GhHome::new();
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let credentials = subspace_core::credentials::Credentials::new(
        &subspace_core::config::Config::default(),
        home.environment(),
        fixture::GhCli(calls.clone()),
    );
    let client = crate::Client::new(Arc::new(credentials)).with_api_url(base);
    let plan = client.prepare_issue_report(
        prepare(&Draft::new("description"), &diagnostics(), &repository()).unwrap(),
    );
    assert_eq!(plan.transport(), TransportKind::Authenticated);
    assert!(matches!(
        client.submit_issue_report(plan).unwrap(),
        Outcome::Created(_)
    ));
    let captured: Vec<_> = requests.try_iter().collect();
    assert_eq!(
        captured[0].authorization,
        "Bearer test-only-active-account-credential"
    );
    assert_eq!(
        *calls.lock().unwrap(),
        vec![vec![
            "auth",
            "token",
            "--hostname",
            "github.com",
            "--user",
            "active-test-user"
        ]]
    );
}
