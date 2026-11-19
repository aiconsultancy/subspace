#![cfg(unix)]

use subspace_search::{Document, Policy, Scope, Service, Target};

use super::navigation_hit;

fn scope() -> Scope {
    Scope {
        app: "test-app".into(),
        board: "irc".into(),
        endpoint: "ircs://irc.example.net:6697".into(),
        tenant: "tenant".into(),
        account: "account".into(),
    }
}

fn document() -> Document {
    Document {
        id: "message".into(),
        title: "Retained message".into(),
        body: "Searchable body".into(),
        metadata: String::new(),
        parent_id: Some("chat".into()),
        kind: "message".into(),
        url: None,
        navigation: "chat/message".into(),
        payload: None,
        refreshed_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64,
    }
}

fn service(root: &std::path::Path) -> Service {
    Service::open(root.to_owned(), "test-app".into(), vec!["irc".into()]).unwrap()
}

fn populate(service: &Service) -> Target {
    service
        .set_policy(
            "irc",
            Policy {
                enabled: true,
                ..Policy::default()
            },
        )
        .unwrap();
    let scope = scope();
    service.activate(&scope).unwrap();
    service.upsert(&scope, &[document()]).unwrap();
    Target {
        scope,
        id: "message".into(),
    }
}

#[cfg(unix)]
#[test]
fn clear_between_background_read_and_native_dispatch_blocks_the_result() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().canonicalize().unwrap();
    let source = service(&path);
    let target = populate(&source);
    let pending = source.read_fenced(&target).unwrap();
    assert!(pending.as_ref().unwrap().permit.valid());
    let other_process = service(&path);
    assert!(pending.as_ref().unwrap().permit.valid());
    other_process.clear("irc").unwrap();
    assert!(navigation_hit(Ok((target, pending))).is_none());
}

#[cfg(unix)]
#[test]
fn valid_cached_navigation_preserves_the_exact_target_and_document() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().canonicalize().unwrap();
    let source = service(&path);
    let target = populate(&source);
    let cached = source.read_fenced(&target).unwrap();
    let (hit, permit) = navigation_hit(Ok((target.clone(), cached))).unwrap();
    assert_eq!(hit.target, target);
    assert_eq!(hit.document.body, "Searchable body");
    assert!(permit.valid());
    assert!(navigation_hit(Err("unavailable".into())).is_none());
}
