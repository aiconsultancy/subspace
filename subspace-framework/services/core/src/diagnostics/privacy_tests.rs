use chrono::{TimeZone as _, Utc};

use super::{
    BuildIdentity, ErrorDetails, LaunchKind, LocalDiagnostics, RedactionContext, RuntimeDetails,
};

#[test]
fn redacts_private_values_in_every_untrusted_text_field() {
    let planted = "TEST_ONLY_ENVIRONMENT_VALUE";
    let snapshot = LocalDiagnostics::new(BuildIdentity::new("subspace", "1.2.3"), "1.16.0")
        .with_runtime(RuntimeDetails::new(
            "/opt/app",
            LaunchKind::Terminal,
            planted,
            None,
        ))
        .with_redaction(RedactionContext::new(None, [planted.into()]));
    let error = ErrorDetails::new(planted, planted, planted).with_code(planted);
    let copy = snapshot.local_copy(Some(&error));
    assert!(!copy.markdown().contains(planted), "{}", copy.markdown());
    assert!(copy.markdown().contains("- App version: 1.2.3"));
}

#[test]
fn unix_home_replacement_matches_whole_paths_not_neighbouring_users() {
    let context = RedactionContext::new(Some("/Users/test".into()), []);
    let snapshot = LocalDiagnostics::default().with_redaction(context);
    let message = concat!(
        "Open /Users/test/a and '/Users/test/b', (/Users/test). Home /Users/test. ",
        "Keep /Users/test-other/a /Users/testing/a /prefix/Users/test/a unchanged."
    );
    let copy = snapshot.local_copy(Some(&ErrorDetails::new("Read failed", message, "read")));
    assert!(copy.markdown().contains("Open ~/a and '~/b', (~). Home ~."));
    assert!(
        copy.markdown()
            .contains("/Users/test-other/a /Users/testing/a /prefix/Users/test/a unchanged.")
    );
}

#[test]
fn windows_home_replacement_handles_case_slashes_spaces_and_boundaries() {
    let context = RedactionContext::new(Some(r"C:\Users\Test Person\".into()), []);
    let snapshot = LocalDiagnostics::new(BuildIdentity::new("subspace", "1"), "1")
        .with_runtime(RuntimeDetails::new(
            r"c:\users\test person\App\app.exe",
            LaunchKind::Desktop,
            "login shell C:/Users/TEST PERSON/bin/bash.exe",
            Some(r"C:\Users\Test Person\bin\az.cmd".into()),
        ))
        .with_redaction(context);
    let message = r"Open C:\Users\Test Person\config. Keep C:\Users\Test Person-other\x unchanged.";
    let copy = snapshot.local_copy(Some(&ErrorDetails::new("Read failed", message, "read")));
    assert!(copy.markdown().contains(r"Open ~\config."));
    assert!(
        copy.markdown()
            .contains(r"C:\Users\Test Person-other\x unchanged.")
    );
    assert!(copy.markdown().contains(r"- Executable: ~\App\app.exe"));
    assert!(
        copy.markdown()
            .contains("- PATH source: login shell ~/bin/bash.exe")
    );
    assert!(copy.markdown().contains(r"- Azure CLI: ~\bin\az.cmd"));
}

#[test]
fn local_copy_removes_credentials_urls_environment_assignments_and_full_paths() {
    // Readable, non-working fixtures only. There are no real credentials here.
    let full_path = "/test-only/first:/test-only/second:/test-only/third";
    let message = format!(
        "Could not launch Azure CLI. {full_path} PATH = '{full_path}' CUSTOM = \"test spaced value\" https://test-user:test-proxy-password@proxy.invalid:8080/a socks5://proxy.invalid:9 Authorization: Bearer test-only-nonworking-credential"
    );
    let snapshot =
        LocalDiagnostics::default().with_redaction(RedactionContext::new(None, [full_path.into()]));
    let copy = snapshot.local_copy(Some(&ErrorDetails::new(
        "Azure CLI missing",
        message,
        "start",
    )));
    assert!(
        copy.markdown()
            .contains("- Message: Could not launch Azure CLI.")
    );
    for forbidden in [
        full_path,
        "test spaced value",
        "test-proxy-password",
        "proxy.invalid",
        "test-only-nonworking-credential",
    ] {
        assert!(!copy.markdown().contains(forbidden), "{}", copy.markdown());
    }
}

#[test]
fn about_copy_has_no_error_and_legacy_errors_label_observation_time() {
    let snapshot = LocalDiagnostics::default();
    assert!(
        !snapshot
            .local_copy(None)
            .markdown()
            .contains("## Error details")
    );
    let legacy = ErrorDetails::new("Legacy failure", "Try again.", "check");
    assert!(
        snapshot
            .local_copy(Some(&legacy))
            .markdown()
            .contains("- Observed at (UTC):")
    );
    let at = Utc.with_ymd_and_hms(2026, 11, 9, 12, 34, 56).unwrap();
    let error = ErrorDetails::new("Legacy failure", "Try again.", "check").with_observed_at(at);
    let copy = snapshot.local_copy(Some(&error));
    assert!(
        copy.markdown()
            .contains("- Observed at (UTC): 2026-11-09T12:34:56Z")
    );
    assert!(!copy.markdown().contains("Occurred at"));
    assert!(copy.markdown().contains("not found on the app's PATH"));
    assert!(copy.markdown().contains("- Launch: unknown"));
}

#[test]
fn terminal_and_desktop_launches_keep_their_distinct_classifications() {
    for (launch, expected) in [
        (LaunchKind::Terminal, "terminal"),
        (LaunchKind::Desktop, "desktop"),
    ] {
        let snapshot = LocalDiagnostics::default().with_runtime(RuntimeDetails::new(
            "/opt/app",
            launch,
            "inherited",
            None,
        ));
        assert!(
            snapshot
                .local_copy(None)
                .markdown()
                .contains(&format!("- Launch: {expected}"))
        );
    }
}

#[test]
fn bounds_untrusted_unicode_fields_and_prevents_markdown_field_injection() {
    let error = ErrorDetails::new("Safe\n- Injected: no", "é".repeat(5_000), "check");
    let copy = LocalDiagnostics::default().local_copy(Some(&error));
    assert!(copy.markdown().contains("- Title: Safe - Injected: no\n"));
    assert!(!copy.markdown().contains("\n- Injected:"));
    assert!(copy.markdown().contains(" [truncated]"));
    assert!(copy.markdown().len() < 10_000);
}

#[test]
fn never_renders_a_path_list_as_the_executable() {
    let snapshot = LocalDiagnostics::default().with_runtime(RuntimeDetails::new(
        r"C:\test-only\bin;D:\test-only\other",
        LaunchKind::Desktop,
        "inherited",
        None,
    ));
    let error = ErrorDetails::new("Start failed", "Try again.", "start");
    let copy = snapshot.local_copy(Some(&error));
    assert!(copy.markdown().contains("- Message: Try again."));
    assert!(!copy.markdown().contains("test-only"));
}

#[test]
fn home_values_become_tilde_and_private_environment_paths_remain_hidden() {
    let home = "/Users/Test Person";
    let private_value = "/Users/Test Person/private-test-environment-file";
    let snapshot = LocalDiagnostics::default().with_redaction(RedactionContext::new(
        Some(home.into()),
        [home.into(), private_value.into()],
    ));
    let message = format!("Read {home}/config but never include {private_value}.");
    let copy = snapshot.local_copy(Some(&ErrorDetails::new("Read failed", message, "read")));
    assert!(copy.markdown().contains("Read ~/config"));
    assert!(!copy.markdown().contains("private-test-environment-file"));
    assert!(!copy.markdown().contains(home));
}

#[test]
fn username_values_do_not_destroy_home_paths_and_normalized_private_paths_stay_hidden() {
    let home = "/Users/fixture";
    let snapshot = LocalDiagnostics::default().with_redaction(RedactionContext::new(
        Some(home.into()),
        [
            "fixture".into(),
            home.into(),
            "/Users/fixture/project".into(),
            "/Users/fixture/private-test-file".into(),
        ],
    ));
    let error = ErrorDetails::new(
        "Read failed",
        "Read /Users/fixture/cache. Working dir /Users/fixture/project. File /Users/fixture/private-test-file. User fixture.",
        "read",
    ).with_code("Home is /Users/fixture").with_private_values(["fixture".into()]);
    let copy = snapshot.local_copy(Some(&error));
    assert!(
        copy.markdown()
            .contains("Read ~/cache. Working dir [redacted]. File [redacted]. User [redacted].")
    );
    assert!(copy.markdown().contains("- Code: Home is ~\n"));
    assert!(!copy.markdown().contains("private-test-file"));
    assert!(!copy.markdown().contains("/Users/"));
}

#[test]
fn private_error_values_append_and_match_whole_identities_not_words() {
    let error = ErrorDetails::new(
        "Cannot connect to test-private-host",
        "Check subscription permissions for b in rg. Retry after signing in.",
        "connect test-private-host",
    )
    .with_private_values(["test-private-host".into(), "b".into()])
    .with_private_values(["rg".into(), String::new()])
    .with_code("test-private-host");
    let copy = LocalDiagnostics::default().local_copy(Some(&error));
    assert!(copy.markdown().contains(
        "Check subscription permissions for [redacted] in [redacted]. Retry after signing in."
    ));
    assert!(!copy.markdown().contains("test-private-host"));
}

#[test]
fn quoted_credentials_and_incomplete_private_keys_never_leave_the_error_block() {
    // Synthetic, deliberately incomplete data. Build its marker at runtime so
    // the private-key hook does not mistake this redaction fixture for a key.
    let incomplete_key = format!(
        "Request failed. -----BEGIN {}-----\nTEST_ONLY incomplete key material",
        "PRIVATE KEY"
    );
    let cases = [
        (
            r#"Request failed. {"client_secret": "TEST_ONLY quoted secret words"}"#,
            "quoted secret words",
        ),
        (
            "Request failed. password: 'TEST_ONLY spaced password words'",
            "spaced password words",
        ),
        (incomplete_key.as_str(), "incomplete key material"),
    ];
    for (message, forbidden) in cases {
        let error = ErrorDetails::new("Request failed", message, "connect");
        let copy = LocalDiagnostics::default().local_copy(Some(&error));
        assert!(copy.markdown().contains("- Message: Request failed."));
        assert!(!copy.markdown().contains(forbidden), "{}", copy.markdown());
    }
}
