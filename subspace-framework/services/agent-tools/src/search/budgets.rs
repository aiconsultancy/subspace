use serde_json::json;

use super::{MAX_RESPONSE_BYTES, bounded_response};

#[test]
fn budget_includes_envelope_and_sources_not_only_hits() {
    let response = bounded_response(json!({"hits":["x".repeat(MAX_RESPONSE_BYTES - 20)],"sources":["source"],"untrusted":true,"truncated":false})).unwrap();
    assert!(serde_json::to_vec(&response).unwrap().len() <= MAX_RESPONSE_BYTES);
    assert_eq!(response["truncated"], true);
    assert!(response["hits"].as_array().unwrap().is_empty());
    let sources = bounded_response(
        json!({"hits":[],"sources":["x".repeat(MAX_RESPONSE_BYTES)],"truncated":false}),
    )
    .unwrap();
    assert!(sources["sources"].as_array().unwrap().is_empty());
    assert!(bounded_response(json!({"body":"x".repeat(MAX_RESPONSE_BYTES)})).is_err());
}
