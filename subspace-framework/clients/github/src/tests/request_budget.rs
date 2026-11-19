use super::{client, serve, token};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[test]
fn empty_and_inaccessible_pr_sections_skip_optional_enrichment_requests() {
    for nodes in [json!([]), json!([null, null])] {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let url = serve(Arc::new(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
            (
                200,
                json!({"data":{"search":{"issueCount":0,"nodes":nodes}}}).to_string(),
            )
        }));
        let response = client(url, &token("tok"))
            .section("ann", "review", None)
            .unwrap();
        assert!(response.items.is_empty());
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "empty sections need only the row query"
        );
    }
}

#[test]
fn a_failed_row_query_does_not_launch_optional_enrichment() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let url = serve(Arc::new(move |_| {
        seen.fetch_add(1, Ordering::SeqCst);
        (403, r#"{"message":"API rate limit exceeded"}"#.into())
    }));
    assert!(
        client(url, &token("tok"))
            .section("ann", "review", None)
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
