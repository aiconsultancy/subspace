use super::{document, enable, query, setup};
use subspace_search::{Scope, Service};

#[test]
fn another_instance_clear_cancels_queued_activation() {
    let (root, service, scope) = setup();
    enable(&service, &scope);
    let ticket = service.prepare_activation(&scope).unwrap();
    let other =
        Service::open(root.path().join("search"), "app".into(), vec!["irc".into()]).unwrap();
    other.clear("irc").unwrap();
    assert!(service.activate_prepared(&ticket).is_err());
    assert!(other.search(&query("hello")).unwrap().hits.is_empty());
}

#[test]
fn same_second_older_fetch_cannot_overwrite_newer_response() {
    let (root, service, scope) = setup();
    enable(&service, &scope);
    let older = service.lease(&scope).unwrap();
    let other =
        Service::open(root.path().join("search"), "app".into(), vec!["irc".into()]).unwrap();
    let newer = other.lease(&scope).unwrap();
    let newest = document("same", "newer");
    let obsolete = subspace_search::Document {
        body: "older".into(),
        ..newest.clone()
    };
    other
        .upsert_leased(&newer, std::slice::from_ref(&newest))
        .unwrap();
    service.upsert_leased(&older, &[obsolete]).unwrap();
    assert_eq!(service.search(&query("newer")).unwrap().hits.len(), 1);
    let latest = service.lease(&scope).unwrap();
    service
        .upsert_leased(
            &latest,
            &[subspace_search::Document {
                body: "latest".into(),
                ..newest
            }],
        )
        .unwrap();
    assert_eq!(service.search(&query("latest")).unwrap().hits.len(), 1);
    service.remove_leased(&older, &["same".into()]).unwrap();
    assert_eq!(service.search(&query("latest")).unwrap().hits.len(), 1);
}

#[test]
fn current_selection_is_shared_between_independent_instances() {
    let (root, service, scope) = setup();
    enable(&service, &scope);
    service.upsert(&scope, &[document("old", "hello")]).unwrap();
    let other =
        Service::open(root.path().join("search"), "app".into(), vec!["irc".into()]).unwrap();
    service
        .activate(&Scope {
            account: "new-account".into(),
            ..scope
        })
        .unwrap();
    assert!(other.search(&query("hello")).unwrap().hits.is_empty());
}

#[test]
fn explicit_transition_blocks_other_instance_but_routine_lookup_failure_preserves_offline() {
    let (root, service, scope) = setup();
    enable(&service, &scope);
    service.upsert(&scope, &[document("old", "hello")]).unwrap();
    let other =
        Service::open(root.path().join("search"), "app".into(), vec!["irc".into()]).unwrap();
    let _routine = service.begin_activation("irc").unwrap();
    assert_eq!(other.search(&query("hello")).unwrap().hits.len(), 1);
    let _switch = service.begin_transition("irc").unwrap();
    assert!(other.search(&query("hello")).unwrap().hits.is_empty());
    assert!(
        other
            .read(&subspace_search::Target {
                scope,
                id: "old".into()
            })
            .unwrap()
            .is_none()
    );
}
