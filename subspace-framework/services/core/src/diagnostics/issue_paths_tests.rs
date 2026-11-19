use super::{BuildIdentity, LaunchKind, LocalDiagnostics, RedactionContext, RuntimeDetails};

#[test]
fn common_numeric_environment_values_do_not_rewrite_error_codes() {
    let snapshot = LocalDiagnostics::default().with_redaction(RedactionContext::new(
        None,
        ["1".into(), "2".into(), "TEST_ONLY_PRIVATE_ENV".into()],
    ));
    let error = super::ErrorDetails::new(
        "HTTP 401",
        "HTTP 401 after 2 attempts; password=1; TEST_ONLY_PRIVATE_ENV",
        "request",
    )
    .with_code("401");
    let copy = snapshot.local_copy(Some(&error));
    assert!(copy.markdown().contains("- Title: HTTP 401\n"));
    assert!(copy.markdown().contains("- Code: 401\n"));
    assert!(copy.markdown().contains("HTTP 401 after 2 attempts;"));
    assert!(!copy.markdown().contains("password=1"));
    assert!(!copy.markdown().contains("TEST_ONLY_PRIVATE_ENV"));
}

#[test]
fn issue_paths_share_home_redaction_without_short_environment_values_corrupting_paths() {
    let cases = [
        (
            "/Users/fixture",
            "/Users/fixture/Apps/v1/app",
            "login shell /Users/fixture/bin/zsh",
            "/Users/fixture/bin/az",
            "/Users/fixture/My Config/config.json",
            "~/Apps/v1/app",
            "login shell ~/bin/zsh",
            "~/bin/az",
            "~/My Config/config.json",
        ),
        (
            r"C:\Users\fixture",
            r"c:\users\fixture\Apps\v1\app.exe",
            r"login shell C:\Users\fixture\bin\bash.exe",
            r"C:\Users\fixture\bin\az.cmd",
            r"C:\Users\fixture\My Config\config.json",
            r"~\Apps\v1\app.exe",
            r"login shell ~\bin\bash.exe",
            r"~\bin\az.cmd",
            r"~\My Config\config.json",
        ),
    ];
    for (
        home,
        exe,
        source,
        az,
        config,
        expected_exe,
        expected_source,
        expected_az,
        expected_config,
    ) in cases
    {
        let runtime = RuntimeDetails::new(exe, LaunchKind::Desktop, source, Some(az.into()));
        let snapshot = LocalDiagnostics::new(BuildIdentity::new("subspace", "1.2.3"), "1")
            .with_runtime(runtime)
            .with_redaction(RedactionContext::new(
                Some(home.into()),
                ["1".into(), "fixture".into(), home.into()],
            ));
        let paths = snapshot.issue_paths(Some(config));
        assert_eq!(
            paths.fields().collect::<Vec<_>>(),
            vec![
                ("Executable", expected_exe),
                ("Launch", "desktop"),
                ("PATH source", expected_source),
                ("Azure CLI", expected_az),
                ("Configuration file", expected_config),
            ]
        );
    }
}

#[test]
fn issue_paths_scrub_opaque_environment_values_even_inside_named_paths() {
    let planted = "TEST_ONLY_OPAQUE_ENV_VALUE";
    let snapshot = LocalDiagnostics::default()
        .with_runtime(RuntimeDetails::new(
            format!("/Users/fixture/{planted}/app"),
            LaunchKind::Terminal,
            format!("login shell /Users/fixture/{planted}/zsh"),
            Some(format!("/opt/{planted}/az")),
        ))
        .with_redaction(RedactionContext::new(
            Some("/Users/fixture".into()),
            [planted.into(), "1".into()],
        ));
    let config = format!("/Users/fixture/{planted}/config.json");
    let paths = snapshot.issue_paths(Some(&config));
    for (_, value) in paths.fields() {
        assert!(!value.contains(planted), "{value}");
        assert!(!value.contains("/Users/fixture"), "{value}");
    }
    assert!(!format!("{paths:?}").contains(planted));
}

#[test]
fn issue_paths_never_render_full_path_values_proxy_urls_or_quoted_assignments() {
    let cases = [
        ("/Users/fixture", "/Users/fixture/bin:/usr/test-only-bin"),
        (
            r"C:\Users\fixture",
            r"C:\Users\fixture\bin;D:\test-only-bin",
        ),
        (
            "/Users/fixture",
            "/Users/fixture/bin:relative-test-only-bin",
        ),
        (
            r"C:\Users\fixture",
            r"C:\Users\fixture\bin;relative-test-only-bin",
        ),
        ("/Users/fixture", "/Users/fixture/bin:"),
    ];
    for (home, full_path) in cases {
        let snapshot = LocalDiagnostics::default()
            .with_runtime(RuntimeDetails::new(full_path, LaunchKind::Terminal,
                "inherited (CUSTOM_ENV = 'TEST_ONLY spaced environment value' https://test-user:test-proxy-password@proxy.invalid:9)",
                Some("https://test-user:test-proxy-password@proxy.invalid/az".into())))
            .with_redaction(RedactionContext::new(Some(home.into()), [full_path.into()]));
        let paths = snapshot.issue_paths(Some(full_path));
        let text = format!("{paths:?}");
        for forbidden in [
            "test-only-bin",
            "test-proxy-password",
            "proxy.invalid",
            "TEST_ONLY spaced environment value",
        ] {
            assert!(!text.contains(forbidden), "{text}");
        }
        assert_eq!(paths.fields().count(), 5);
        assert_eq!(paths.fields().next().unwrap().1, "[redacted]");
        assert_eq!(paths.fields().last().unwrap().1, "[redacted]");
    }
}

#[test]
fn named_executable_shell_and_azure_cli_environment_paths_remain_allowed() {
    let snapshot = LocalDiagnostics::default()
        .with_runtime(RuntimeDetails::new(
            "/opt/test-app",
            LaunchKind::Terminal,
            "login shell /bin/zsh",
            Some("/usr/bin/az".into()),
        ))
        .with_redaction(RedactionContext::new(
            None,
            [
                "/opt/test-app".into(),
                "/bin/zsh".into(),
                "/usr/bin/az".into(),
            ],
        ));
    assert_eq!(
        snapshot.issue_paths(None).fields().collect::<Vec<_>>(),
        vec![
            ("Executable", "/opt/test-app"),
            ("Launch", "terminal"),
            ("PATH source", "login shell /bin/zsh"),
            ("Azure CLI", "/usr/bin/az"),
        ]
    );
}

#[test]
fn local_copy_and_issue_paths_share_sanitized_runtime_fields() {
    let planted = "TEST_ONLY_OPAQUE_PATH_VALUE";
    let cases = [
        (Some(format!("/opt/{planted}/az")), "/opt/[redacted]/az"),
        (None, "not found on the app's PATH"),
    ];
    for (az_path, expected_az) in cases {
        let snapshot = LocalDiagnostics::new(BuildIdentity::new("subspace", "1.2.3"), "1")
            .with_runtime(RuntimeDetails::new(
                format!("/Users/fixture/{planted}/v1/app"),
                LaunchKind::Desktop,
                format!("login shell /Users/fixture/{planted}/zsh"),
                az_path,
            ))
            .with_redaction(RedactionContext::new(
                Some("/Users/fixture".into()),
                [planted.into(), "1".into(), "fixture".into(), "found".into()],
            ));
        let local = snapshot.local_copy(None);
        let paths = snapshot.issue_paths(None);
        let fields = paths.fields().collect::<Vec<_>>();
        assert_eq!(fields.len(), 4);
        assert!(fields.contains(&("Azure CLI", expected_az)));
        assert!(!local.markdown().contains(planted), "{}", local.markdown());
        assert!(local.markdown().contains("- App version: 1.2.3\n"));
        for (name, value) in fields {
            assert!(!value.contains(planted), "{name}: {value}");
            assert!(local.markdown().contains(&format!("- {name}: {value}")));
        }
    }
}
