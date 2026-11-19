use super::{BrowserMode, Diagnostics, Draft, ErrorKind, IssueRepository, IssueType, prepare};

mod http;
mod paths;

/// A fictional repository: the Framework names no real one.
fn repository() -> IssueRepository {
    IssueRepository::parse("example-org/example-app").unwrap()
}

fn diagnostics() -> Diagnostics {
    Diagnostics::new("subspace", "1.2.3")
}

#[test]
fn description_is_required_but_its_whitespace_is_preserved() {
    assert_eq!(
        prepare(&Draft::new(" \n\t"), &diagnostics(), &repository())
            .unwrap_err()
            .kind(),
        ErrorKind::DescriptionRequired
    );
    let report = prepare(
        &Draft::new("  first line  \nsecond\n"),
        &diagnostics(),
        &repository(),
    )
    .unwrap();
    assert_eq!(report.title(), "first line");
    assert_eq!(report.issue_type(), IssueType::Unknown);
    assert_eq!(
        report.body(),
        "## Description\n\n  first line  \nsecond\n\n\n## Type\n\nUnknown\n\n## Diagnostics\n\n- Product: subspace\n- App version: 1.2.3"
    );
}

#[test]
fn derived_title_uses_first_nonblank_line_and_unicode_character_limit() {
    let description = format!("\n \n{}\nignored title", "界".repeat(257));
    let report = prepare(&Draft::new(description), &diagnostics(), &repository()).unwrap();
    assert_eq!(report.title(), "界".repeat(256));
    assert!(report.body().contains(&"界".repeat(257)));
    let explicit = Draft::new("description").with_title("explicit\ntitle");
    assert_eq!(
        prepare(&explicit, &diagnostics(), &repository())
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidTitle
    );
    let too_long = Draft::new("description").with_title("é".repeat(257));
    assert_eq!(
        prepare(&too_long, &diagnostics(), &repository())
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidTitle
    );
    assert!(
        prepare(
            &Draft::new("x").with_title("é".repeat(256)),
            &diagnostics(),
            &repository()
        )
        .is_ok()
    );
}

#[test]
fn explicit_type_and_steps_keep_exact_user_text_and_request_only_known_label_names() {
    for (kind, name, label) in [
        (IssueType::Bug, "Bug", "bug"),
        (IssueType::Ux, "UX", "ux"),
        (IssueType::FeatureRequest, "Feature request", "enhancement"),
        (IssueType::Other, "Other", "other"),
        (IssueType::Unknown, "Unknown", "unknown"),
    ] {
        let draft = Draft::new("**exact** description\n")
            .with_title("My title")
            .with_issue_type(kind)
            .with_steps("1. do this\n\n  unchanged  ");
        let report = prepare(&draft, &diagnostics(), &repository()).unwrap();
        assert_eq!(report.title(), "My title");
        assert_eq!(report.requested_labels(), [label, "source:subspace"]);
        assert!(report.body().contains(&format!("## Type\n\n{name}")));
        assert!(report.body().contains("**exact** description\n\n\n"));
        assert!(
            report
                .body()
                .contains("## Steps to reproduce\n\n1. do this\n\n  unchanged  ")
        );
    }
}

#[test]
fn diagnostics_project_only_explicit_safe_fields() {
    let context = serde_json::json!({
        "product_id":"subspace", "app_version":"1.2.3", "channel":"nightly",
        "commit":"abc123", "os":"macos", "arch":"aarch64", "board":"github",
        "theme":"one-dark", "icon_size":"small", "config":"TEST_ONLY_CONFIG_SENTINEL",
        "path":"/test-only/private/path", "hostname":"private.test.invalid",
        "tunnel":"TEST_ONLY_TUNNEL_SENTINEL", "token":"TEST_ONLY_NONWORKING_TOKEN",
        "environment":"TEST_ONLY_ENV_SENTINEL", "logs":"TEST_ONLY_LOG_SENTINEL"
    });
    let text = |name| context[name].as_str().unwrap();
    let safe = Diagnostics::new(text("product_id"), text("app_version"))
        .with_build(text("channel"), text("commit"))
        .with_platform(text("os"), text("arch"))
        .with_active_board(text("board"))
        .with_appearance(text("theme"), text("icon_size"));
    let report = prepare(&Draft::new("ordinary description"), &safe, &repository()).unwrap();
    assert_eq!(
        report.body(),
        "## Description\n\nordinary description\n\n## Type\n\nUnknown\n\n## Diagnostics\n\n- Product: subspace\n- App version: 1.2.3\n- Build channel: nightly\n- Commit: abc123\n- OS: macos\n- Architecture: aarch64\n- Active board: github\n- Theme: one-dark\n- Icon size: small"
    );
    for forbidden in [
        "config",
        "path",
        "hostname",
        "tunnel",
        "token",
        "environment",
        "logs",
    ] {
        assert!(!report.body().contains(text(forbidden)));
    }
    assert!(
        prepare(
            &Draft::new("text"),
            &Diagnostics::new("p\nInjected: secret", "1"),
            &repository()
        )
        .is_err()
    );
}

#[test]
fn user_authored_sensitive_looking_text_is_not_silently_rewritten() {
    let user_text = "TEST_ONLY_NONWORKING_TOKEN /test/path private.test.invalid";
    let report = prepare(&Draft::new(user_text), &diagnostics(), &repository()).unwrap();
    assert!(report.body().contains(user_text));
    assert_eq!(
        prepare(
            &Draft::new("a".repeat(65_536)),
            &diagnostics(),
            &repository()
        )
        .unwrap_err()
        .kind(),
        ErrorKind::BodyTooLong
    );
}

#[test]
fn unicode_url_budget_trims_diagnostics_without_truncating_user_text() {
    let draft = Draft::new("界".repeat(840))
        .with_title("é & ? #")
        .with_steps("Keep these steps exactly.\n");
    let safe = diagnostics()
        .with_build("nightly", "a".repeat(128))
        .with_platform("macos", "aarch64")
        .with_appearance("one-dark", "small");
    let report = prepare(&draft, &safe, &repository()).unwrap();
    let fallback = report.browser_fallback();
    assert_eq!(fallback.editor_mode(), BrowserMode::Prefilled);
    assert!(fallback.diagnostics_omitted() > 0);
    assert!(fallback.url().len() <= 8_000);
    assert!(fallback.url().contains("%C3%A9"));
    let url = url::Url::parse(fallback.url()).unwrap();
    let values: std::collections::HashMap<_, _> = url.query_pairs().collect();
    assert_eq!(values["body"], fallback.body());
    assert_eq!(values["title"], "é & ? #");
    assert!(fallback.body().contains(draft.description()));
    assert!(fallback.body().contains(draft.steps()));
}

#[test]
fn oversized_browser_report_requires_exact_full_body_clipboard_and_blank_editor() {
    let report = prepare(
        &Draft::new("界".repeat(1_000)).with_steps("Never truncate steps"),
        &diagnostics(),
        &repository(),
    )
    .unwrap();
    let fallback = report.browser_fallback();
    assert_eq!(fallback.editor_mode(), BrowserMode::ClipboardRequired);
    assert_eq!(fallback.clipboard_body(), Some(report.body()));
    assert_eq!(fallback.body(), report.body());
    assert_eq!(
        fallback.url(),
        "https://github.com/example-org/example-app/issues/new?template=blank"
    );
}

#[test]
fn only_positive_issue_urls_in_the_named_repository_are_safe_to_open() {
    let repository = repository();
    assert!(repository.is_created_issue_url("https://github.com/example-org/example-app/issues/1"));
    for url in [
        "http://github.com/example-org/example-app/issues/1",
        "https://github.com/other/repo/issues/1",
        "https://github.com/example-org/example-app/issues/0",
        "https://github.com/example-org/example-app/issues/-1",
        "https://github.com/example-org/example-app/issues/1?next=evil",
        "https://github.com/example-org/example-app/issues/1#fragment",
        "https://github.com/example-org/example-app/issues/1/",
        "https://github.com@evil.invalid/example-org/example-app/issues/1",
        "https://github.com/example-org/example-app/pull/1",
    ] {
        assert!(!repository.is_created_issue_url(url), "{url}");
    }
}

#[test]
fn a_repository_is_owner_slash_name_and_nothing_else() {
    let repository = IssueRepository::parse(" example-org/example.app_2 ").unwrap();
    assert_eq!(repository.as_str(), "example-org/example.app_2");
    assert_eq!(
        repository.new_issue_url(),
        "https://github.com/example-org/example.app_2/issues/new?template=blank"
    );
    for refused in [
        "",
        "example-org",
        "example-org/",
        "/example-app",
        "example-org/example-app/issues",
        "-example/example-app",
        "example-/example-app",
        "example org/example-app",
        "example-org/..",
        "example-org/.",
        "example-org/app?x=1",
        "https://github.com/example-org/example-app",
    ] {
        assert_eq!(IssueRepository::parse(refused), None, "{refused}");
    }
}
