use super::*;

#[test]
fn api_tokens_are_scoped_cached_and_never_persisted() {
    let (manager, store, provider) = fixture(None);
    manager.restore().unwrap();
    std::thread::scope(|scope| {
        for _ in 0..6 {
            let manager = manager.clone();
            scope.spawn(move || {
                let token = manager.token_for("example-api").unwrap();
                assert_eq!(
                    token.token.expose(),
                    "api://00000000-0000-0000-0000-000000000004/API.Access"
                );
                assert_eq!(token.account.username, "developer@example.test");
            });
        }
    });
    let issues = manager.token_for("issues").unwrap();
    assert_eq!(
        issues.token.expose(),
        "api://00000000-0000-0000-0000-000000000005/API.Access"
    );
    assert_eq!(provider.count.load(Ordering::SeqCst), 3);
    assert_eq!(
        *provider.scopes.lock().unwrap(),
        [
            "",
            "api://00000000-0000-0000-0000-000000000004/API.Access",
            "api://00000000-0000-0000-0000-000000000005/API.Access"
        ]
    );
    let stored = store.load().unwrap().unwrap();
    assert!(!stored.expose().contains("API.Access"));
    manager.sign_out().unwrap();
    assert_eq!(
        manager.token_for("example-api").err(),
        Some(Error::SignInRequired)
    );
}

#[test]
fn unknown_resources_fail_before_network_or_keychain_access() {
    let (manager, _, provider) = fixture(None);
    assert_eq!(
        manager.token_for("https://attacker.example.test").err(),
        Some(Error::Unconfigured)
    );
    assert_eq!(
        manager.start_for("unknown").err(),
        Some(Error::Unconfigured)
    );
    assert_eq!(provider.count.load(Ordering::SeqCst), 0);
}

#[test]
fn expiry_refreshes_only_the_required_resource() {
    let (manager, _, provider) = fixture(None);
    manager.restore().unwrap();
    manager.token_for("example-api").unwrap();
    manager.token_for("issues").unwrap();
    manager
        .cache
        .lock()
        .unwrap()
        .access
        .get_mut("example-api")
        .unwrap()
        .1 = std::time::Instant::now();
    manager.token_for("example-api").unwrap();
    manager.token_for("issues").unwrap();
    assert_eq!(provider.count.load(Ordering::SeqCst), 4);
}

#[test]
fn resource_renewal_skips_a_cached_token_with_insufficient_lifetime() {
    let (manager, _, provider) = fixture(None);
    manager.restore().unwrap();
    manager.token_for("example-api").unwrap();
    manager
        .cache
        .lock()
        .unwrap()
        .access
        .get_mut("example-api")
        .unwrap()
        .1 = std::time::Instant::now() + Duration::from_secs(5);
    manager.token_for("example-api").unwrap();
    assert_eq!(provider.count.load(Ordering::SeqCst), 2);
    manager
        .token_with_validity("example-api", Duration::from_secs(180))
        .unwrap();
    assert_eq!(provider.count.load(Ordering::SeqCst), 3);
}

#[test]
fn resource_renewal_catches_wall_clock_expiry_even_if_the_monotonic_timer_is_behind() {
    let (manager, _, provider) = fixture(None);
    manager.restore().unwrap();
    manager.token_for("example-api").unwrap();
    manager
        .cache
        .lock()
        .unwrap()
        .access
        .get_mut("example-api")
        .unwrap()
        .2 = std::time::SystemTime::now() - Duration::from_secs(1);
    manager.token_for("example-api").unwrap();
    assert_eq!(provider.count.load(Ordering::SeqCst), 3);
}

#[test]
fn accepting_a_new_account_discards_all_previous_service_tokens() {
    let (manager, store, _) = fixture(None);
    manager.restore().unwrap();
    manager.token_for("example-api").unwrap();
    let mut session = super::super::decode(&store.load().unwrap().unwrap()).unwrap();
    session.account.id = "00000000-0000-0000-0000-000000000006".into();
    manager
        .accept(
            Granted {
                session,
                access: Secret::new("new-identity-token"),
                expires_at: std::time::Instant::now() + Duration::from_secs(3600),
            },
            &crate::Cancel::default(),
            manager.generation(),
            "",
        )
        .unwrap();
    let cache = manager.cache.lock().unwrap();
    assert_eq!(cache.access.len(), 1);
    assert!(!cache.access.contains_key("example-api"));
}
