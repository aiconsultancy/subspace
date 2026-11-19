use super::{document, enable, query, setup};
use subspace_search::{Policy, Service};

#[test]
fn previous_schema_rebuilds_stored_documents_once() {
    let (root, service, scope) = setup();
    drop(service);
    let path = root.path().join("search/617070/697263.sqlite");
    write_legacy_fixture(&path, &scope);
    for _ in 0..2 {
        let reopened =
            Service::open(root.path().join("search"), "app".into(), vec!["irc".into()]).unwrap();
        assert_eq!(
            reopened
                .search(&query("migration search"))
                .unwrap()
                .hits
                .len(),
            1
        );
        assert_eq!(
            reopened.search(&query("migration")).unwrap().hits[0]
                .document
                .body,
            "migration searchable"
        );
    }
    let connection = rusqlite::Connection::open(path).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
            .unwrap(),
        3
    );
}

#[test]
fn a_policy_chosen_before_version_three_is_kept_from_a_board_default() {
    let (root, service, scope) = setup();
    drop(service);
    let path = root.path().join("search/617070/697263.sqlite");
    write_legacy_fixture(&path, &scope);
    let reopened =
        Service::open(root.path().join("search"), "app".into(), vec!["irc".into()]).unwrap();
    let offered = Policy {
        enabled: true,
        retention_days: 30,
        ..Policy::default()
    };
    assert!(!reopened.offer_default("irc", offered).unwrap());
    assert_eq!(reopened.policy("irc").unwrap().retention_days, 60);
}

fn write_legacy_fixture(path: &std::path::Path, scope: &subspace_search::Scope) {
    std::fs::remove_file(path).unwrap();
    let connection = rusqlite::Connection::open(path).unwrap();
    connection
        .execute_batch(include_str!("../fixtures/schema-v0.sql"))
        .unwrap();
    connection.execute_batch("PRAGMA user_version=1").unwrap();
    let policy = serde_json::to_string(&Policy {
        enabled: true,
        ..Policy::default()
    })
    .unwrap();
    let key = serde_json::to_string(&scope).unwrap();
    connection
        .execute(
            "INSERT INTO settings VALUES(1,?1,?2,0)",
            rusqlite::params![policy, key],
        )
        .unwrap();
    let stored = document("old", "migration searchable");
    connection
        .execute(
            "INSERT INTO documents VALUES(1,?1,'old',?2,?3)",
            rusqlite::params![
                key,
                stored.refreshed_at,
                serde_json::to_string(&stored).unwrap()
            ],
        )
        .unwrap();
    // Deliberately missing FTS row proves rebuilding reads stored JSON.
}

#[test]
fn newer_schema_is_rejected_without_modification() {
    let (root, service, _scope) = setup();
    drop(service);
    let path = root.path().join("search/617070/697263.sqlite");
    let connection = rusqlite::Connection::open(path).unwrap();
    connection.execute_batch("PRAGMA user_version=99").unwrap();
    assert!(Service::open(root.path().join("search"), "app".into(), vec!["irc".into()]).is_err());
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
            .unwrap(),
        99
    );
}

#[test]
fn only_last_query_term_is_a_prefix() {
    let (_root, service, scope) = setup();
    enable(&service, &scope);
    service
        .upsert(&scope, &[document("one", "deployment migration")])
        .unwrap();
    assert_eq!(
        service
            .search(&query("deployment migr"))
            .unwrap()
            .hits
            .len(),
        1
    );
    assert!(
        service
            .search(&query("deploy migration"))
            .unwrap()
            .hits
            .is_empty()
    );
    assert!(
        service
            .search(&query("unrelated migr"))
            .unwrap()
            .hits
            .is_empty()
    );
}
