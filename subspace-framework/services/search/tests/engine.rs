use subspace_search::{Document, Policy, Query, Ranking, Scope, Service};

#[path = "cases/facets.rs"]
mod facets;
#[cfg(unix)]
#[path = "cases/fences.rs"]
mod fences;
#[path = "cases/isolation.rs"]
mod isolation;
#[path = "cases/migration.rs"]
mod migration;
#[path = "cases/quota.rs"]
mod quota;
#[path = "cases/ranking.rs"]
mod ranking;
#[path = "cases/records.rs"]
mod records;
#[path = "cases/review.rs"]
mod review;
#[path = "cases/selection.rs"]
mod selection;
#[path = "cases/storage.rs"]
mod storage;

fn setup() -> (tempfile::TempDir, Service, Scope) {
    let root = tempfile::tempdir().unwrap();
    let service =
        Service::open(root.path().join("search"), "app".into(), vec!["irc".into()]).unwrap();
    let scope = Scope {
        app: "app".into(),
        board: "irc".into(),
        endpoint: "https://example.test".into(),
        tenant: "tenant".into(),
        account: "account".into(),
    };
    (root, service, scope)
}

fn document(id: &str, body: &str) -> Document {
    Document {
        id: id.into(),
        title: "Title".into(),
        body: body.into(),
        metadata: String::new(),
        parent_id: None,
        kind: "message".into(),
        url: None,
        navigation: format!("chat/{id}"),
        refreshed_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64,
        payload: Some("{\"cached\":true}".into()),
    }
}

fn query(text: &str) -> Query {
    Query {
        text: text.into(),
        boards: vec![],
        limit: 100,
        ranking: Ranking::Bm25,
    }
}

fn enable(service: &Service, scope: &Scope) {
    service
        .set_policy(
            "irc",
            Policy {
                enabled: true,
                ..Policy::default()
            },
        )
        .unwrap();
    service.activate(scope).unwrap();
}
