use subspace_contracts::issues::IssueScope;

fn scope() -> IssueScope {
    IssueScope::new(
        "subspace-api",
        "Subspace API",
        "https://api.example.test/api",
    )
    .with_project_keys(["SUB", "OPS"])
}

#[test]
fn navigation_binds_the_current_profile_without_an_organisation() {
    let url = scope().navigation_url(
        "https://api.example.test/api/v1/issues/SUB-1?profile=old&orgId=old&extra=value",
    );
    assert_eq!(
        url.as_deref(),
        Some("https://api.example.test/api/v1/issues/SUB-1?profile=subspace-api")
    );
}

#[test]
fn navigation_rejects_other_origins_and_embedded_credentials() {
    for url in [
        "https://elsewhere.example.test/api/v1/issues/SUB-1",
        "http://api.example.test/api/v1/issues/SUB-1",
        "https://api.example.test:8443/api/v1/issues/SUB-1",
        "https://user@api.example.test/api/v1/issues/SUB-1",
        "https://user:password@api.example.test/api/v1/issues/SUB-1",
        "/issues/SUB-1",
    ] {
        assert!(scope().navigation_url(url).is_none(), "{url}");
    }
}

#[test]
fn navigation_round_trips_the_exact_api_route_identifier_and_profile() {
    let scope = scope();
    let url = scope
        .navigation_url("https://api.example.test/api/v1/issues/OPS-42")
        .unwrap();
    assert_eq!(
        url,
        "https://api.example.test/api/v1/issues/OPS-42?profile=subspace-api"
    );
    assert_eq!(scope.navigation_identifier(&url).as_deref(), Some("OPS-42"));
    for query in [
        "",
        "?profile=other",
        "?profile=subspace-api&profile=subspace-api",
        "?profile=subspace-api&extra=1",
    ] {
        let url = format!("https://api.example.test/api/v1/issues/SUB-1{query}");
        assert!(scope.navigation_identifier(&url).is_none(), "{url}");
    }
}

#[test]
fn navigation_rejects_urls_the_responding_board_cannot_open() {
    let scope = scope();
    for path in [
        "/issues/SUB-1",
        "/other/v1/issues/SUB-1",
        "/api/v1/issues/OTHER-1",
        "/api/v1/issues/SUB-0",
        "/api/v1/issues/SUB-1/activity",
        "/api/v1/issues/SUB-1/",
        "/api/v1/issues/SUB-1#fragment",
        "/api/v1/issues/%53UB-1",
    ] {
        let url = format!("https://api.example.test{path}");
        assert!(scope.navigation_url(&url).is_none(), "{url}");
        assert!(
            scope
                .navigation_identifier(&format!("{url}?profile=subspace-api"))
                .is_none(),
            "{url}"
        );
    }
}
