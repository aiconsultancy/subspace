use serde_json::json;
use subspace_search::{Policy, Scope, Service, Target};
use tempfile::TempDir;

use super::{call, catalog, document, fixture, scoped_catalog};

fn two_boards() -> (TempDir, Service, Scope) {
    let (root, service, scope) = fixture();
    drop(service);
    let service = Service::open(
        root.path().into(),
        "test".into(),
        vec!["irc".into(), "hidden".into()],
    )
    .unwrap();
    let hidden = Scope {
        board: "hidden".into(),
        ..scope.clone()
    };
    service
        .set_policy(
            "hidden",
            Policy {
                enabled: true,
                ..Policy::default()
            },
        )
        .unwrap();
    service.activate(&hidden).unwrap();
    service
        .upsert(&hidden, &[document("needle".into())])
        .unwrap();
    service
        .upsert(&scope, &[document("needle".into())])
        .unwrap();
    (root, service, hidden)
}

#[tokio::test]
async fn catalog_allowlist_constrains_queries_citations_and_source_statuses() {
    let (_root, service, hidden) = two_boards();
    let tools = catalog(service);
    let result = call(&tools, "search_content", json!({"query":"needle"}))
        .await
        .unwrap();
    assert_eq!(result["hits"].as_array().unwrap().len(), 1);
    assert_eq!(result["sources"].as_array().unwrap().len(), 1);
    assert_eq!(result["hits"][0]["board"], "irc");
    assert_eq!(result["sources"][0]["board"], "irc");
    assert!(
        call(
            &tools,
            "search_content",
            json!({"query":"needle","boards":["hidden"]})
        )
        .await
        .is_err()
    );
    let citation = subspace_search::link(&Target {
        scope: hidden,
        id: "message".into(),
    });
    assert!(
        call(&tools, "read_search_result", json!({"citation":citation}))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn empty_participation_never_falls_back_to_all_boards() {
    let (_root, service, _hidden) = two_boards();
    let no_participants = scoped_catalog(service.clone(), &["nonparticipating"]);
    assert!(
        call(
            &no_participants,
            "search_content",
            json!({"query":"needle"})
        )
        .await
        .is_err()
    );
    assert!(scoped_catalog(service, &[]).get("search_content").is_none());
}
