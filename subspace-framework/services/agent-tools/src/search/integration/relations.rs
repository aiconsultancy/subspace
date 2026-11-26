use serde_json::json;
use subspace_search::{IndexMetadata, Reference, ReferenceIdentity, ReferenceMatch};

use super::{call, catalog, document, fixture};

#[tokio::test]
async fn filtered_search_and_related_items_keep_untrusted_bounded_results() {
    let (_root, service, scope) = fixture();
    let mut retained = document("needle".into());
    retained.kind = "pull-request".into();
    service.upsert(&scope, &[retained]).unwrap();
    let reference = ReferenceIdentity::github("owner/repo", 42).unwrap();
    service
        .index_metadata(
            &scope,
            "message",
            &IndexMetadata {
                project: Some("owner/repo".into()),
                references: vec![Reference {
                    identity: reference.clone(),
                    relation_type: "pull-request".into(),
                    matched: ReferenceMatch::Exact,
                }],
            },
        )
        .unwrap();
    let tools = catalog(service);
    let filtered = call(
        &tools,
        "search_content",
        json!({
            "query":"needle", "kind":"pull-request", "project":"owner/repo",
            "account":"selected", "parent_id":"chat",
        }),
    )
    .await
    .unwrap();
    assert_eq!(filtered["hits"].as_array().unwrap().len(), 1);
    assert_eq!(filtered["untrusted"], true);
    let related = call(
        &tools,
        "related_items",
        json!({
            "reference": reference.as_str(), "boards":["irc"], "relation_types":["pull-request"]
        }),
    )
    .await
    .unwrap();
    assert_eq!(related["hits"].as_array().unwrap().len(), 1);
    assert_eq!(related["hits"][0]["match"], json!("Exact"));
    assert_eq!(related["untrusted"], true);
    assert!(serde_json::to_vec(&related).unwrap().len() <= crate::search::MAX_RESPONSE_BYTES);
}

#[tokio::test]
async fn related_items_accept_a_registered_reference_kind() {
    subspace_search::register_reference_kind("example-ref").unwrap();
    let (_root, service, scope) = fixture();
    service
        .upsert(&scope, &[document("needle".into())])
        .unwrap();
    let reference = ReferenceIdentity::of_kind("example-ref", &["42"]).unwrap();
    let metadata = IndexMetadata {
        project: None,
        references: vec![Reference {
            identity: reference.clone(),
            relation_type: "work-item".into(),
            matched: ReferenceMatch::Exact,
        }],
    };
    service
        .index_metadata(&scope, "message", &metadata)
        .unwrap();
    let tools = catalog(service);
    let related = call(
        &tools,
        "related_items",
        json!({"reference": reference.as_str(), "boards":["irc"]}),
    )
    .await
    .unwrap();
    assert_eq!(related["hits"].as_array().unwrap().len(), 1);
    let refused = call(
        &tools,
        "related_items",
        json!({"reference": "example-unknown:1"}),
    )
    .await;
    assert!(
        refused
            .unwrap_err()
            .to_string()
            .contains("Invalid reference identity")
    );
}
