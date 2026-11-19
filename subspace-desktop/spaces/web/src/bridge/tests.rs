use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde_json::json;
use subspace_framework::core::{
    Secret,
    config::web_boards::{WebBoard, WebBoardSource},
};

use super::{
    Bridge, Effect,
    protocol::{Message, Operation},
};
use crate::manifest::Permission;

fn fixture() -> (tempfile::TempDir, WebBoard, Bridge) {
    let root = tempfile::tempdir().unwrap();
    let manifest = json!({"name": "Notes", "version": "1", "permissions": ["settings", "notifications", "badge", "context"],
        "settings": [{"key": "limit", "label": "Limit", "type": "number", "default": 10},
            {"key": "token", "label": "Token", "type": "secret"}]});
    std::fs::write(
        root.path().join("subspace-board.json"),
        manifest.to_string(),
    )
    .unwrap();
    let config = WebBoard {
        id: "notes".into(),
        title: "Notes".into(),
        source: WebBoardSource::Folder(root.path().into()),
        settings: BTreeMap::from([
            ("token".into(), json!("accidentally-in-settings")),
            ("unknown".into(), json!("x")),
        ]),
        secrets: BTreeMap::from([("token".into(), Secret::new("secret"))]),
        permissions: BTreeMap::new(),
    };
    let bridge = Bridge::new(&config).unwrap();
    (root, config, bridge)
}

fn grant(config: &mut WebBoard, bridge: &Bridge, permission: Permission) {
    config
        .permissions
        .insert(bridge.manifest.grant_key(&bridge.root, permission), true);
}

#[test]
fn permissions_require_user_grants_for_this_manifest_and_folder() {
    let (root, mut config, mut bridge) = fixture();
    let now = Instant::now();
    assert!(
        bridge
            .call(&Operation::Settings, &config, false, now)
            .is_err()
    );
    grant(&mut config, &bridge, Permission::Settings);
    let (value, _) = bridge
        .call(&Operation::Settings, &config, false, now)
        .unwrap();
    assert_eq!(value, json!({"limit": 10.0}));
    std::fs::write(
        root.path().join("subspace-board.json"),
        r#"{"name":"Notes","version":"2","permissions":["settings"]}"#,
    )
    .unwrap();
    let changed = Bridge::new(&config).unwrap();
    assert!(!changed.allows(Permission::Settings, &config));
    assert!(
        bridge
            .call(&Operation::Settings, &config, false, now)
            .is_err()
    );
    config.source = WebBoardSource::Folder(root.path().join("elsewhere"));
    assert!(!bridge.allows(Permission::Settings, &config));
}

#[test]
fn both_sender_and_current_page_must_have_the_exact_app_origin() {
    let (_root, _, bridge) = fixture();
    let origin = &bridge.origin;
    assert!(bridge.trusted(&format!("{origin}/x"), &format!("{origin}/y")));
    assert!(!bridge.trusted("https://evil.example", origin));
    assert!(!bridge.trusted(origin, "https://evil.example"));
    assert!(!bridge.trusted("subspace://other/", origin));
}

#[test]
fn notifications_need_a_channel_and_are_rate_limited() {
    let (_root, mut config, mut bridge) = fixture();
    grant(&mut config, &bridge, Permission::Notifications);
    let op = Operation::Notify(Message {
        title: "New note".into(),
        body: "Ready".into(),
    });
    let now = Instant::now();
    assert!(bridge.call(&op, &config, false, now).is_err());
    assert!(matches!(
        bridge.call(&op, &config, true, now).unwrap().1,
        Effect::Notify(_)
    ));
    assert!(bridge.call(&op, &config, true, now).is_err());
    assert!(
        bridge
            .call(&op, &config, true, now + Duration::from_secs(5))
            .is_ok()
    );
}

#[test]
fn badge_and_context_are_bounded_and_revocation_is_immediate() {
    let (_root, mut config, mut bridge) = fixture();
    grant(&mut config, &bridge, Permission::Badge);
    let now = Instant::now();
    assert!(
        bridge
            .call(
                &Operation::Badge(Some("123456789".into())),
                &config,
                false,
                now
            )
            .is_err()
    );
    assert!(
        bridge
            .call(&Operation::Badge(Some("3".into())), &config, false, now)
            .is_ok()
    );
    assert_eq!(bridge.badge.as_deref(), Some("3"));
    config.permissions.clear();
    assert!(
        bridge
            .call(&Operation::Badge(None), &config, false, now)
            .is_err()
    );
    assert!(!bridge.allows(Permission::Badge, &config));
    assert!(
        bridge
            .call(&Operation::Context(None), &config, false, now)
            .is_err()
    );
}

#[test]
fn badge_and_context_do_not_survive_a_new_document() {
    let (_root, mut config, mut bridge) = fixture();
    grant(&mut config, &bridge, Permission::Badge);
    bridge.enter_document(1);
    bridge
        .call(
            &Operation::Badge(Some("3".into())),
            &config,
            false,
            Instant::now(),
        )
        .unwrap();
    bridge.enter_document(1);
    assert_eq!(bridge.badge.as_deref(), Some("3"));
    bridge.enter_document(2);
    assert!(bridge.badge.is_none());
    assert!(bridge.context.is_none());
}
