use crate::{Document, Policy, Scope, Service, Target, revision::Mutation};

fn scope() -> Scope {
    Scope {
        app: "app".into(),
        board: "board".into(),
        endpoint: "example".into(),
        tenant: String::new(),
        account: "account".into(),
    }
}

fn setup() -> (tempfile::TempDir, Service, Target) {
    let root = tempfile::tempdir().unwrap();
    let service = Service::open(
        root.path().join("search"),
        "app".into(),
        vec!["board".into()],
    )
    .unwrap();
    service
        .set_policy(
            "board",
            Policy {
                enabled: true,
                ..Policy::default()
            },
        )
        .unwrap();
    service.activate(&scope()).unwrap();
    service
        .upsert(
            &scope(),
            &[Document {
                id: "1".into(),
                title: "title".into(),
                body: "body".into(),
                metadata: String::new(),
                parent_id: None,
                kind: "test".into(),
                url: None,
                navigation: "1".into(),
                refreshed_at: crate::now(),
                payload: None,
            }],
        )
        .unwrap();
    (
        root,
        service,
        Target {
            scope: scope(),
            id: "1".into(),
        },
    )
}

#[test]
fn reader_cannot_mint_valid_permit_from_old_sql_snapshot_during_writer_transaction() {
    let (root, service, target) = setup();
    let reader = Service::open(
        root.path().join("search"),
        "app".into(),
        vec!["board".into()],
    )
    .unwrap();
    let mut stores = service.stores().unwrap();
    let store = stores.get_mut("board").unwrap();
    let transaction = store
        .connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let mutation = Mutation::begin(&store.revision);
    transaction.execute("DELETE FROM documents", []).unwrap();
    // DELETE journal readers still see the pre-write committed document.
    assert!(reader.read(&target).unwrap().is_some());
    assert!(reader.read_fenced(&target).unwrap().is_none());
    transaction.commit().unwrap();
    drop(mutation);
    assert!(reader.read_fenced(&target).unwrap().is_none());
}

#[test]
fn child_exits_during_mutation() {
    let Ok(root) = std::env::var("SUBSPACE_SEARCH_CRASH_CHILD_ROOT") else {
        assert!(std::env::var("SUBSPACE_SEARCH_CRASH_CHILD_ROOT").is_err());
        return;
    };
    let service = Service::open(
        std::path::PathBuf::from(root).join("search"),
        "app".into(),
        vec!["board".into()],
    )
    .unwrap();
    let mut stores = service.stores().unwrap();
    let store = stores.get_mut("board").unwrap();
    let transaction = store
        .connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let _mutation = Mutation::begin(&store.revision);
    transaction.execute("DELETE FROM documents", []).unwrap();
    assert!(store.revision.busy());
    std::process::exit(0);
}

#[test]
fn crashed_writer_fails_closed_until_database_recovery_and_revision_reset() {
    let (root, service, target) = setup();
    let before = service.read_fenced(&target).unwrap().unwrap();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "revision::tests::child_exits_during_mutation"])
        .env("SUBSPACE_SEARCH_CRASH_CHILD_ROOT", root.path())
        .status()
        .unwrap();
    assert!(status.success());
    assert!(!before.permit.valid());
    assert!(service.read_fenced(&target).unwrap().is_none());
    let recovered = Service::open(
        root.path().join("search"),
        "app".into(),
        vec!["board".into()],
    )
    .unwrap();
    let after = recovered.read_fenced(&target).unwrap().unwrap();
    assert_eq!(after.document.body, "body");
    assert!(after.permit.valid());
    assert!(!before.permit.valid());
}
