use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{FailureCategory, SDK_VERSION, TurnContext, TurnFailure, categorize};

/// A `session.error` payload shaped like the SDK's `SessionErrorData`.
fn payload(error_type: &str, status: Option<i32>, remediation: Option<&str>) -> Value {
    let mut data = json!({"errorType": error_type, "message": "fixture message"});
    if let Some(status) = status {
        data["statusCode"] = json!(status);
    }
    if let Some(remediation) = remediation {
        data["remediation"] = json!(remediation);
    }
    data
}

fn category_of(data: &Value) -> FailureCategory {
    TurnFailure::from_session_error(data, &TurnContext::default(), None).category
}

pub(super) fn turn() -> TurnContext {
    TurnContext {
        sent: Instant::now().checked_sub(Duration::from_secs(41)),
        attachments: 2,
        delta_received: false,
        model: Some("fixture-model".into()),
        effort: Some("high".into()),
        submitted: Default::default(),
    }
}

#[test]
fn each_category_comes_from_the_fields_the_sdk_sends() {
    let cases = [
        (payload("query", None, None), FailureCategory::NoResponse),
        (
            payload("authentication", None, None),
            FailureCategory::SignIn,
        ),
        (
            payload("authorization", Some(403), None),
            FailureCategory::SignIn,
        ),
        (
            payload("model_call", None, Some("switch_account")),
            FailureCategory::SignIn,
        ),
        (
            payload("model_call", Some(401), None),
            FailureCategory::SignIn,
        ),
        (
            payload("rate_limit", Some(429), None),
            FailureCategory::Limit,
        ),
        (payload("quota", None, None), FailureCategory::Limit),
        (
            payload("model_call", Some(402), None),
            FailureCategory::Limit,
        ),
        (payload("policy", None, None), FailureCategory::NotAllowed),
        (
            payload("context_limit", Some(400), None),
            FailureCategory::ContextFull,
        ),
        (
            payload("model_call", Some(503), None),
            FailureCategory::Network,
        ),
        (
            payload("model_call", Some(407), None),
            FailureCategory::Network,
        ),
        (payload("execution", None, None), FailureCategory::Unknown),
        (payload("query", Some(500), None), FailureCategory::Unknown),
    ];
    for (data, expected) in cases {
        assert_eq!(category_of(&data), expected, "{data}");
    }
}

#[test]
fn the_error_type_wins_over_the_status_and_the_remediation() {
    assert_eq!(
        categorize("rate_limit", Some(401), Some("sign_in")),
        FailureCategory::Limit
    );
    assert_eq!(
        categorize("query", Some(503), Some("show_account")),
        FailureCategory::SignIn,
        "a sign-in remediation wins over the status"
    );
    assert_eq!(categorize("", None, None), FailureCategory::Unknown);
}

#[test]
fn a_payload_without_fields_keeps_them_absent() {
    let failure = TurnFailure::from_session_error(&json!({}), &TurnContext::default(), None);
    assert_eq!(failure.category, FailureCategory::Unknown);
    assert_eq!(failure.error_type, "runtime");
    assert_eq!(
        failure.message,
        "No error details were supplied by the SDK."
    );
    assert_eq!(failure.status_code, None);
    assert_eq!(failure.provider_call_id, None);
    assert_eq!(failure.elapsed, None, "no turn ran");
    assert_eq!(failure.runtime_version, None);
}

#[test]
fn every_session_error_field_is_kept() {
    let data = json!({
        "errorType": "rate_limit",
        "message": "Rate limited",
        "statusCode": 429,
        "errorCode": "user_model_rate_limited",
        "providerCallId": "00000000-0000-0000-0000-provider0001",
        "serviceRequestId": "00000000-0000-0000-0000-service00001",
        "originatingMessageId": "00000000-0000-0000-0000-message00001",
        "remediation": "show_account",
        "url": "https://example.invalid/limits",
        "eligibleForAutoSwitch": true
    });
    let mut context = turn();
    context.delta_received = true;
    let failure = TurnFailure::from_session_error(&data, &context, Some("1.0.105"));
    assert_eq!(failure.category, FailureCategory::Limit);
    assert_eq!(failure.error_type, "rate_limit");
    assert_eq!(failure.message, "Rate limited");
    assert_eq!(failure.status_code, Some(429));
    assert_eq!(
        failure.error_code.as_deref(),
        Some("user_model_rate_limited")
    );
    assert_eq!(
        failure.provider_call_id.as_deref(),
        Some("00000000-0000-0000-0000-provider0001")
    );
    assert_eq!(
        failure.service_request_id.as_deref(),
        Some("00000000-0000-0000-0000-service00001")
    );
    assert_eq!(
        failure.originating_message_id.as_deref(),
        Some("00000000-0000-0000-0000-message00001")
    );
    assert_eq!(failure.remediation.as_deref(), Some("show_account"));
    assert_eq!(
        failure.url.as_deref(),
        Some("https://example.invalid/limits")
    );
    assert_eq!(failure.model.as_deref(), Some("fixture-model"));
    assert_eq!(failure.effort.as_deref(), Some("high"));
    assert_eq!(failure.runtime_version.as_deref(), Some("1.0.105"));
    assert_eq!(failure.sdk_version, SDK_VERSION);
    assert!(
        failure
            .elapsed
            .is_some_and(|elapsed| elapsed >= Duration::from_secs(41))
    );
    assert!(failure.delta_received);
    assert_eq!(failure.attachments, 2);
}

#[test]
fn details_list_present_fields_and_omit_absent_ones() {
    let failure = TurnFailure::from_session_error(&payload("query", None, None), &turn(), None);
    let text = failure.details_text();
    for line in [
        "Type: query",
        "Message: fixture message",
        "Model: fixture-model",
        "Reasoning effort: high",
        "SDK: 1.0.34",
        "Reply started: no",
        "Attachments: 2",
    ] {
        assert!(text.lines().any(|row| row == line), "{line} in {text}");
    }
    assert!(text.contains("Elapsed: 41."), "{text}");
    assert!(text.contains("Time: "), "{text}");
    for absent in [
        "Status:",
        "Error code:",
        "Request ID:",
        "Runtime:",
        "Message ID:",
    ] {
        assert!(!text.contains(absent), "{absent} in {text}");
    }
    assert_eq!(text.lines().count(), failure.details().len());
}

#[test]
fn a_stopped_runtime_names_its_own_error_type() {
    let stopped =
        TurnFailure::stopped(FailureCategory::Stopped, "closed", &turn(), Some("1.0.105"));
    assert_eq!(stopped.error_type, "runtime_stopped");
    assert_eq!(stopped.model.as_deref(), Some("fixture-model"));
    let silent = TurnFailure::stopped(FailureCategory::NotResponding, "silent", &turn(), None);
    assert_eq!(silent.error_type, "runtime_not_responding");
    assert!(silent.category.needs_reconnect());
    assert!(!FailureCategory::NoResponse.needs_reconnect());
}

#[test]
fn every_category_has_a_title_an_action_and_a_round_tripping_slug() {
    for category in FailureCategory::ALL {
        assert!(!category.title().is_empty());
        assert!(category.action().ends_with('.'), "{category:?}");
        assert_eq!(FailureCategory::from_slug(category.slug()), Some(category));
    }
    assert_eq!(FailureCategory::from_slug("missing"), None);
}

#[test]
fn the_sdk_version_matches_the_cargo_pin() {
    let manifest = include_str!("../../Cargo.toml");
    assert!(
        manifest.contains(&format!("github-copilot-sdk = \"={SDK_VERSION}\"")),
        "update SDK_VERSION with the github-copilot-sdk pin"
    );
}

#[test]
fn a_turn_learns_its_model_and_whether_a_reply_started() {
    let mut context = TurnContext::default();
    context.begin(3, Vec::new());
    context.observe(
        &json!({"type": "assistant.usage", "data": {"model": "m1", "reasoningEffort": "low"}}),
    );
    assert_eq!(context.model.as_deref(), Some("m1"));
    assert_eq!(context.effort.as_deref(), Some("low"));
    context.observe(&json!({"type": "assistant.message", "data": {"content": ""}}));
    assert!(!context.delta_received, "an empty message is no reply");
    context.observe(&json!({"type": "assistant.reasoning_delta", "data": {"deltaContent": "x"}}));
    assert!(context.delta_received);
    context.observe(&json!({"type": "session.model_change", "data": {"newModel": "m2"}}));
    assert_eq!(context.model.as_deref(), Some("m2"));
    assert_eq!(context.effort, None);
    context.finish();
    assert_eq!(context.sent, None);
    assert_eq!(context.attachments, 3);
}
