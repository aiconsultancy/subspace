use super::{document, enable, query, setup};
use subspace_search::{Policy, Target};

#[test]
fn unicode_literals_all_words_edits_deletes_and_stable_navigation() {
    let (_root, service, scope) = setup();
    enable(&service, &scope);
    service
        .upsert(
            &scope,
            &[
                document("1", "café 東京 alpha beta"),
                document("2", "alpha only"),
            ],
        )
        .unwrap();
    let response = service.search(&query("café 東京")).unwrap();
    assert_eq!(response.hits.len(), 1);
    assert_eq!(response.hits[0].document.navigation, "chat/1");
    assert_eq!(service.search(&query("alpha beta")).unwrap().hits.len(), 1);
    assert!(
        service
            .search(&query("alpha OR missing"))
            .unwrap()
            .hits
            .is_empty()
    );
    assert!(
        service
            .search(&query("\" * : ( )"))
            .unwrap()
            .hits
            .is_empty()
    );
    service
        .upsert(&scope, &[document("1", "replacement")])
        .unwrap();
    assert!(service.search(&query("café")).unwrap().hits.is_empty());
    assert_eq!(service.search(&query("replacement")).unwrap().hits.len(), 1);
    service.remove(&scope, &["1".into()]).unwrap();
    assert!(
        service
            .read(&Target {
                scope,
                id: "1".into()
            })
            .unwrap()
            .is_none()
    );
}

#[test]
fn quota_counts_real_database_index_and_rejects_batch_atomically() {
    let (_root, service, scope) = setup();
    enable(&service, &scope);
    let used = service.sources().unwrap()[0].bytes;
    assert!(used >= 4096);
    assert!(
        service
            .set_policy(
                "irc",
                Policy {
                    enabled: true,
                    max_bytes: used,
                    ..Policy::default()
                },
            )
            .is_err()
    );
    enable(&service, &scope);
    let oversized = document("large", &"a".repeat(128 * 1024));
    assert!(
        service
            .upsert(&scope, &[document("small", "one"), oversized])
            .is_err()
    );
    assert!(service.search(&query("one")).unwrap().hits.is_empty());
    service.upsert(&scope, &[document("small", "one")]).unwrap();
    assert_eq!(service.sources().unwrap()[0].records, 1);
    assert!(
        service.sources().unwrap()[0]
            .message
            .contains("Partial history")
    );
}

#[test]
fn an_older_fetch_does_not_overwrite_observed_edits() {
    let (_root, service, scope) = setup();
    enable(&service, &scope);
    let edited = document("same", "edited");
    // Derive the older time from the stored edit, not from a second clock
    // read, which can cross a second boundary on a slow runner.
    let mut older = document("same", "obsolete");
    older.refreshed_at = edited.refreshed_at - 1;
    service.upsert(&scope, &[edited]).unwrap();
    service.upsert(&scope, &[older]).unwrap();
    assert_eq!(service.search(&query("edited")).unwrap().hits.len(), 1);
    assert!(service.search(&query("obsolete")).unwrap().hits.is_empty());
}

#[test]
fn removal_cancels_older_fetch_and_clear_cancels_older_removal() {
    let (_root, service, scope) = setup();
    enable(&service, &scope);
    let lease = service.lease(&scope).unwrap();
    service
        .upsert_leased(&lease, &[document("same", "hello")])
        .unwrap();
    service.remove_leased(&lease, &["same".into()]).unwrap();
    assert!(
        service
            .upsert_leased(&lease, &[document("same", "hello")])
            .is_err()
    );
    service.clear("irc").unwrap();
    service.activate(&scope).unwrap();
    service.upsert(&scope, &[document("same", "new")]).unwrap();
    assert!(service.remove_leased(&lease, &["same".into()]).is_err());
    assert_eq!(service.search(&query("new")).unwrap().hits.len(), 1);
}
