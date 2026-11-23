use super::{document, enable, query, setup};
use subspace_search::{Policy, Query, Scope, Service};

#[test]
fn disabled_by_default_and_persists_verified_scope_for_headless_restart() {
    let (root, service, scope) = setup();
    assert_eq!(service.policy("irc").unwrap(), Policy::default());
    assert!(service.upsert(&scope, &[document("1", "hello")]).is_err());
    enable(&service, &scope);
    service.upsert(&scope, &[document("1", "hello")]).unwrap();
    drop(service);
    let reopened =
        Service::open(root.path().join("search"), "app".into(), vec!["irc".into()]).unwrap();
    let hits = reopened.search(&query("hello")).unwrap().hits;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].target.scope, scope);
    assert_eq!(
        reopened.read(&hits[0].target).unwrap().unwrap().payload,
        Some("{\"cached\":true}".into())
    );
}

#[test]
fn account_app_isolation_and_clear_lease_fences() {
    let (root, service, scope) = setup();
    enable(&service, &scope);
    let lease = service.lease(&scope).unwrap();
    service.upsert(&scope, &[document("1", "hello")]).unwrap();
    let other = Scope {
        account: "other".into(),
        ..scope.clone()
    };
    service.activate(&other).unwrap();
    assert!(service.search(&query("hello")).unwrap().hits.is_empty());
    assert!(
        service
            .upsert_leased(&lease, &[document("late", "hello")])
            .is_err()
    );
    service.activate(&scope).unwrap();
    assert_eq!(service.search(&query("hello")).unwrap().hits.len(), 1);
    service.clear("irc").unwrap();
    assert!(
        service
            .upsert(&scope, &[document("late", "hello")])
            .is_err()
    );
    service.activate(&scope).unwrap();
    assert!(
        service
            .upsert_leased(&lease, &[document("late", "hello")])
            .is_err()
    );
    let other_app = Service::open(
        root.path().join("search"),
        "different".into(),
        vec!["irc".into()],
    )
    .unwrap();
    assert!(other_app.search(&query("hello")).unwrap().hits.is_empty());
    assert!(other_app.activate(&scope).is_err());
}

#[test]
fn expiry_disable_and_invalid_query_fail_closed() {
    let (_root, service, scope) = setup();
    enable(&service, &scope);
    let mut expired = document("expired", "hello");
    expired.refreshed_at -= 61 * 86400;
    service
        .upsert(&scope, &[expired, document("new", "hello")])
        .unwrap();
    assert_eq!(service.search(&query("hello")).unwrap().hits.len(), 1);
    let lease = service.lease(&scope).unwrap();
    service.set_policy("irc", Policy::default()).unwrap();
    assert!(service.search(&query("hello")).unwrap().hits.is_empty());
    enable(&service, &scope);
    assert!(
        service
            .upsert_leased(&lease, &[document("late", "hello")])
            .is_err()
    );
    assert!(service.search(&query(&"a".repeat(2049))).is_err());
    assert!(
        service
            .search(&Query {
                limit: 101,
                ..query("hello")
            })
            .is_err()
    );
}

#[test]
fn independent_handles_observe_invalidation_and_service_is_send_sync() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Service>();
    let (root, service, scope) = setup();
    enable(&service, &scope);
    let lease = service.lease(&scope).unwrap();
    let other =
        Service::open(root.path().join("search"), "app".into(), vec!["irc".into()]).unwrap();
    let revoker =
        Service::open(root.path().join("search"), "app".into(), vec!["irc".into()]).unwrap();
    revoker.invalidate(&scope).unwrap();
    assert!(
        service
            .upsert_leased(&lease, &[document("late", "hello")])
            .is_err()
    );
    service.activate(&scope).unwrap();
    service.upsert(&scope, &[document("1", "hello")]).unwrap();
    assert_eq!(other.search(&query("hello")).unwrap().hits.len(), 1);
    assert!(service.sweep().is_ok());
}

#[test]
fn a_board_hides_documents_from_every_search_without_deleting_them() {
    let (_root, service, scope) = setup();
    enable(&service, &scope);
    service
        .upsert(
            &scope,
            &[
                document("mine", "launch window"),
                document("theirs", "launch window"),
            ],
        )
        .unwrap();
    let hidden = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let rule = std::sync::Arc::clone(&hidden);
    service
        .set_hidden("irc", move |document| {
            rule.load(std::sync::atomic::Ordering::SeqCst) && document.id == "theirs"
        })
        .unwrap();
    let ids = |service: &Service| -> Vec<String> {
        let mut ids: Vec<String> = service
            .search(&query("launch"))
            .unwrap()
            .hits
            .into_iter()
            .map(|hit| hit.target.id)
            .collect();
        ids.sort();
        ids
    };
    assert_eq!(ids(&service), ["mine"]);
    let shown = service.search(&query("launch")).unwrap().hits;
    hidden.store(false, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(ids(&service), ["mine", "theirs"], "nothing was deleted");
    let kept = service.search(&query("launch")).unwrap().hits;
    hidden.store(true, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(shown.len(), 1);
    assert_eq!(
        kept.iter().filter(|hit| service.shows(hit)).count(),
        1,
        "a result kept from earlier is asked again"
    );
}
