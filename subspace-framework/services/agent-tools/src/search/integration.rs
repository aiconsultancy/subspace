use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use subspace_core::config::Config;
use subspace_search::{Document, Policy, Scope, Service};
use tempfile::TempDir;

use crate::{
    Access, CallMonitor, Catalog, Invocation, Unattended, Unmonitored, catalog_with_help,
    test_support,
};

fn fixture() -> (TempDir, Service, Scope) {
    let root = tempfile::tempdir().unwrap();
    let service = Service::open(root.path().into(), "test".into(), vec!["irc".into()]).unwrap();
    let scope = Scope {
        app: "test".into(),
        board: "irc".into(),
        endpoint: "https://example.test".into(),
        tenant: "tenant".into(),
        account: "selected".into(),
    };
    service
        .set_policy(
            "irc",
            Policy {
                enabled: true,
                ..Policy::default()
            },
        )
        .unwrap();
    service.activate(&scope).unwrap();
    (root, service, scope)
}

fn document(body: String) -> Document {
    Document {
        id: "message".into(),
        title: "ordinary title".into(),
        body,
        metadata: String::new(),
        parent_id: Some("chat".into()),
        kind: "message".into(),
        url: None,
        navigation: "private-navigation".into(),
        refreshed_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64,
        payload: Some("private-payload".into()),
    }
}

fn catalog(service: Service) -> Catalog {
    scoped_catalog(service, &["irc", "nonparticipating"])
}

fn scoped_catalog(service: Service, boards: &[&'static str]) -> Catalog {
    let mut environment = test_support::environment(Config::default());
    environment.search = Some(service);
    let docs = boards
        .iter()
        .map(|id| crate::BoardDoc {
            id,
            title: id,
            markdown: "test",
            images: &[],
        })
        .collect();
    catalog_with_help(&[], docs, &environment).unwrap()
}

async fn call(catalog: &Catalog, tool: &str, arguments: Value) -> Result<Value, String> {
    catalog
        .get(tool)
        .unwrap()
        .call(Invocation {
            arguments,
            approval: Arc::new(Unattended),
            monitor: Arc::new(Unmonitored),
        })
        .await
}

#[tokio::test]
async fn shared_catalog_searches_retained_full_body_and_reads_without_payload() {
    let (root, service, scope) = fixture();
    service
        .upsert(
            &scope,
            &[document(format!("{} needle", "filler ".repeat(2000)))],
        )
        .unwrap();
    drop(service);
    let reopened = Service::open(root.path().into(), "test".into(), vec!["irc".into()]).unwrap();
    let catalog = catalog(reopened);
    for name in ["search_content", "read_search_result"] {
        assert_eq!(catalog.get(name).unwrap().spec().access, Access::Read);
    }
    let result = call(&catalog, "search_content", json!({"query":"needle"}))
        .await
        .unwrap();
    assert_eq!(result["hits"].as_array().unwrap().len(), 1);
    assert!(!result.to_string().contains("private-"));
    let read = call(
        &catalog,
        "read_search_result",
        json!({"citation":result["hits"][0]["open_in_subspace"]}),
    )
    .await
    .unwrap();
    assert_eq!(read["available"], true);
    assert!(!read.to_string().contains("private-"));
    assert!(read["body"].as_str().unwrap().len() <= super::MAX_BODY_BYTES);
}

#[tokio::test]
async fn citations_revalidate_selection_clear_and_disable() {
    let (_root, service, scope) = fixture();
    let tools = catalog(service.clone());
    service
        .upsert(&scope, &[document("needle".into())])
        .unwrap();
    let found = call(&tools, "search_content", json!({"query":"needle"}))
        .await
        .unwrap();
    let args = json!({"citation":found["hits"][0]["open_in_subspace"]});
    let other = Scope {
        account: "other".into(),
        ..scope.clone()
    };
    service.activate(&other).unwrap();
    assert_eq!(
        call(&tools, "read_search_result", args.clone())
            .await
            .unwrap()["available"],
        false
    );
    service.activate(&scope).unwrap();
    assert_eq!(
        call(&tools, "read_search_result", args.clone())
            .await
            .unwrap()["available"],
        true
    );
    service.clear("irc").unwrap();
    assert_eq!(
        call(&tools, "read_search_result", args.clone())
            .await
            .unwrap()["available"],
        false
    );
    service.activate(&scope).unwrap();
    service
        .upsert(&scope, &[document("needle".into())])
        .unwrap();
    service.set_policy("irc", Policy::default()).unwrap();
    assert_eq!(
        call(&tools, "read_search_result", args).await.unwrap()["available"],
        false
    );
}

#[tokio::test]
async fn argument_errors_are_bounded_and_never_echo_input() {
    let (_root, service, _scope) = fixture();
    let tools = catalog(service);
    for arguments in [
        json!({"query":"needle","tenant":"secret"}),
        json!({"query":"needle","database":"secret"}),
        json!({"query":"needle","limit":"secret"}),
        json!({"query":"é".repeat(1025)}),
        json!({"query":"needle","limit":0}),
        json!({"query":"needle","limit":51}),
        json!({"query":"needle","boards":vec!["irc";33]}),
        json!({"query":"needle","boards":["x".repeat(65)]}),
        json!({"query":"secret","boards":["secret"]}),
    ] {
        let error = call(&tools, "search_content", arguments).await.unwrap_err();
        assert!(!error.contains("secret"));
        assert!(error.len() < 200);
    }
    let error = call(
        &tools,
        "search_content",
        json!({"query":"needle","mystery":true}),
    )
    .await
    .unwrap_err();
    assert!(error.contains("mystery"));
    for arguments in [
        json!({"citation":"secret","database":"secret"}),
        json!({"citation":"secret"}),
        json!({"citation":"x".repeat(8193)}),
    ] {
        assert!(
            !call(&tools, "read_search_result", arguments)
                .await
                .unwrap_err()
                .contains("secret")
        );
    }
}

#[tokio::test]
async fn actual_responses_bound_large_metadata_and_unicode_body() {
    let (_root, service, scope) = fixture();
    let documents: Vec<_> = (0..50)
        .map(|id| Document {
            id: id.to_string(),
            title: "title ".repeat(600),
            ..document("needle".into())
        })
        .collect();
    service.upsert(&scope, &documents).unwrap();
    let tools = catalog(service.clone());
    let result = call(
        &tools,
        "search_content",
        json!({"query":"needle","limit":50}),
    )
    .await
    .unwrap();
    assert!(serde_json::to_vec(&result).unwrap().len() <= super::MAX_RESPONSE_BYTES);
    assert_eq!(result["truncated"], true);
    assert!(result["hits"].as_array().unwrap().len() < 50);
    service
        .upsert(&scope, &[document("é".repeat(10000))])
        .unwrap();
    let citation = subspace_search::link(&subspace_search::Target {
        scope,
        id: "message".into(),
    });
    let read = call(&tools, "read_search_result", json!({"citation":citation}))
        .await
        .unwrap();
    assert_eq!(read["body"].as_str().unwrap().len(), super::MAX_BODY_BYTES);
    assert_eq!(read["truncated"], true);
    assert!(serde_json::to_vec(&read).unwrap().len() <= super::MAX_RESPONSE_BYTES);
}

#[tokio::test]
async fn schemas_and_runtime_accept_the_documented_limits() {
    let (_root, service, _scope) = fixture();
    let tools = catalog(service);
    let schema = tools.get("search_content").unwrap().spec().input_schema();
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["properties"]["query"]["maxLength"], 2048);
    assert_eq!(schema["properties"]["boards"]["maxItems"], 32);
    assert_eq!(schema["properties"]["limit"]["maximum"], 50);
    let result = call(
        &tools,
        "search_content",
        json!({"query":"x".repeat(2048),"limit":50,"boards":vec!["irc";32]}),
    )
    .await
    .unwrap();
    assert!(result["hits"].as_array().unwrap().is_empty());
}

mod relations;
mod scopes;

struct CancelAfterStart(AtomicUsize);

impl CallMonitor for CancelAfterStart {
    fn cancelled(&self) -> bool {
        self.0.fetch_add(1, Ordering::SeqCst) > 0
    }
}

#[tokio::test]
async fn cancellation_discards_completed_worker_results() {
    let (_root, service, scope) = fixture();
    service
        .upsert(&scope, &[document("needle".into())])
        .unwrap();
    let tools = catalog(service);
    let result = tools
        .get("search_content")
        .unwrap()
        .call(Invocation {
            arguments: json!({"query":"needle"}),
            approval: Arc::new(Unattended),
            monitor: Arc::new(CancelAfterStart(AtomicUsize::new(0))),
        })
        .await;
    assert_eq!(result.unwrap_err(), "Search cancelled.");
}
