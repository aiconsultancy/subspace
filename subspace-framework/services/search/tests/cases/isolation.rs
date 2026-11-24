use super::{document, enable, query, setup};
use subspace_search::{
    Document, IndexMetadata, Policy, Query, Reference, ReferenceIdentity, ReferenceMatch,
    RelatedQuery, Scope, Service, Target,
};

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

/// A document without a payload, so a search that returns it whole is not
/// truncated.
fn plain(id: &str, body: &str) -> Document {
    Document {
        payload: None,
        ..document(id, body)
    }
}

/// A store holding `z-mine` and the hidden `a-theirs`, both linking pull
/// request 42, with a rule hiding every id that starts with `a-theirs`.
fn hiding(service: &Service, scope: &Scope) -> ReferenceIdentity {
    let identity = ReferenceIdentity::github("owner/repo", 42).unwrap();
    // The hidden one is newest, so it would come first.
    for id in ["z-mine", "a-theirs"] {
        service
            .upsert(scope, &[plain(id, "launch window")])
            .unwrap();
        let metadata = IndexMetadata {
            project: Some("repo".into()),
            references: vec![Reference {
                identity: identity.clone(),
                relation_type: "mentioned-pull-request".into(),
                matched: ReferenceMatch::Exact,
            }],
        };
        service.index_metadata(scope, id, &metadata).unwrap();
    }
    service
        .set_hidden("irc", |document| document.id.starts_with("a-theirs"))
        .unwrap();
    identity
}

#[test]
fn hidden_records_are_neither_related_nor_read_by_their_citation() {
    let (_root, service, scope) = setup();
    enable(&service, &scope);
    let identity = hiding(&service, &scope);
    let related = |limit| {
        service
            .related_items(
                &identity,
                &RelatedQuery {
                    relation_types: vec![],
                    boards: vec![],
                    limit,
                },
            )
            .unwrap()
            .into_iter()
            .map(|item| item.hit.target.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(related(20), ["z-mine"]);
    assert_eq!(related(1), ["z-mine"], "a hidden one takes no place");
    let target = |id: &str| Target {
        scope: scope.clone(),
        id: id.into(),
    };
    assert!(service.read(&target("a-theirs")).unwrap().is_none());
    assert!(service.read(&target("z-mine")).unwrap().is_some());
    #[cfg(unix)]
    {
        assert!(service.read_fenced(&target("a-theirs")).unwrap().is_none());
        assert!(service.read_fenced(&target("z-mine")).unwrap().is_some());
    }
}

#[test]
fn hidden_records_never_crowd_visible_ones_out_of_a_search() {
    let (_root, service, scope) = setup();
    enable(&service, &scope);
    hiding(&service, &scope);
    // More hidden matches than a search reads candidates, each ranking
    // above the visible one.
    for batch in 0..2 {
        let theirs: Vec<_> = (0..80)
            .map(|n| plain(&format!("a-theirs-{batch}-{n:02}"), "launch launch"))
            .collect();
        service.upsert(&scope, &theirs).unwrap();
    }
    let response = service.search(&query("launch")).unwrap();
    let ids: Vec<_> = response
        .hits
        .iter()
        .map(|hit| hit.target.id.as_str())
        .collect();
    assert_eq!(ids, ["z-mine"]);
    assert!(!response.truncated);
}
