use super::{document, enable, setup};
use subspace_search::{CachedRead, Policy, Service, Target};

fn cached_fixture() -> (tempfile::TempDir, Service, Target, CachedRead) {
    let (root, service, scope) = setup();
    enable(&service, &scope);
    service.upsert(&scope, &[document("1", "hello")]).unwrap();
    let target = Target {
        scope: scope.clone(),
        id: "1".into(),
    };
    let cached = service.read_fenced(&target).unwrap().unwrap();
    (root, service, target, cached)
}

#[test]
fn cached_read_is_revoked_across_independent_instances_and_normal_refresh_preserves_it() {
    let (root, service, target, cached) = cached_fixture();
    assert!(cached.permit.valid());
    let other =
        Service::open(root.path().join("search"), "app".into(), vec!["irc".into()]).unwrap();
    let ticket = other.begin_activation("irc").unwrap();
    let activation = other
        .prepare_activation_ticket(&ticket, &target.scope)
        .unwrap();
    other.activate_prepared(&activation).unwrap();
    assert!(cached.permit.valid());
    other.clear("irc").unwrap();
    assert!(!cached.permit.valid());
    assert!(service.read_fenced(&target).unwrap().is_none());
}

#[test]
fn cached_handoff_is_revoked_by_disable_and_account_transition() {
    let (_root, service, target, cached) = cached_fixture();
    service.begin_transition("irc").unwrap();
    assert!(!cached.permit.valid());
    service.activate(&target.scope).unwrap();
    let cached = service.read_fenced(&target).unwrap().unwrap();
    service.set_policy("irc", Policy::default()).unwrap();
    assert!(!cached.permit.valid());
}

#[test]
fn child_clears_cache() {
    let Ok(root) = std::env::var("SUBSPACE_SEARCH_REVISION_CHILD_ROOT") else {
        assert!(std::env::var("SUBSPACE_SEARCH_REVISION_CHILD_ROOT").is_err());
        return;
    };
    let service = Service::open(
        std::path::PathBuf::from(root).join("search"),
        "app".into(),
        vec!["irc".into()],
    )
    .unwrap();
    service.clear("irc").unwrap();
    assert_eq!(service.sources().unwrap()[0].records, 0);
}

#[test]
fn a_separate_process_revokes_an_already_returned_read_permit() {
    let (root, _service, _target, cached) = cached_fixture();
    assert!(cached.permit.valid());
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "fences::child_clears_cache"])
        .env("SUBSPACE_SEARCH_REVISION_CHILD_ROOT", root.path())
        .status()
        .unwrap();
    assert!(status.success());
    assert!(!cached.permit.valid());
}
