use super::{document, enable, query, setup};
use subspace_search::{Policy, Scope};

#[test]
fn queued_activations_cannot_reverse_newer_selection_or_survive_clear() {
    let (_root, service, scope) = setup();
    enable(&service, &scope);
    service.upsert(&scope, &[document("old", "hello")]).unwrap();
    let lease = service.lease(&scope).unwrap();
    let old = service.prepare_activation(&scope).unwrap();
    let other = Scope {
        account: "other".into(),
        ..scope.clone()
    };
    let transition = service.begin_transition("irc").unwrap();
    let new = service
        .prepare_activation_ticket(&transition, &other)
        .unwrap();
    assert!(service.search(&query("hello")).unwrap().hits.is_empty());
    service.activate_prepared(&new).unwrap();
    assert!(service.activate_prepared(&old).is_err());
    assert!(
        service
            .upsert_leased(&lease, &[document("late", "hello")])
            .is_err()
    );
    service.clear("irc").unwrap();
    assert!(service.activate_prepared(&new).is_err());
    let after_clear = service.prepare_activation(&scope).unwrap();
    service.set_policy("irc", Policy::default()).unwrap();
    assert!(service.activate_prepared(&after_clear).is_err());
}

#[test]
fn pending_away_and_back_invalidates_old_fetch_without_database_activation() {
    let (_root, service, scope) = setup();
    enable(&service, &scope);
    let lease = service.lease(&scope).unwrap();
    let transition = service.begin_transition("irc").unwrap();
    service
        .prepare_activation_ticket(
            &transition,
            &Scope {
                account: "other".into(),
                ..scope.clone()
            },
        )
        .unwrap();
    let latest = service.prepare_activation(&scope).unwrap();
    service.activate_prepared(&latest).unwrap();
    assert!(
        service
            .upsert_leased(&lease, &[document("late", "hello")])
            .is_err()
    );
    service.upsert(&scope, &[document("new", "hello")]).unwrap();
    assert_eq!(service.search(&query("hello")).unwrap().hits.len(), 1);
}

#[test]
fn identity_fetch_tickets_are_cancelled_by_clear_and_newer_observations() {
    let (_root, service, scope) = setup();
    enable(&service, &scope);
    let old = service.begin_activation("irc").unwrap();
    service.clear("irc").unwrap();
    assert!(service.prepare_activation_ticket(&old, &scope).is_err());
    let old = service.begin_activation("irc").unwrap();
    let new = service.begin_activation("irc").unwrap();
    let selected = service.prepare_activation_ticket(&new, &scope).unwrap();
    service.activate_prepared(&selected).unwrap();
    assert!(service.prepare_activation_ticket(&new, &scope).is_err());
    assert!(service.prepare_activation_ticket(&old, &scope).is_err());
    service.upsert(&scope, &[document("new", "hello")]).unwrap();
    assert_eq!(service.search(&query("hello")).unwrap().hits.len(), 1);
}
