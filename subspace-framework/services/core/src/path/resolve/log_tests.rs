use std::path::PathBuf;

use chrono::{TimeZone as _, Utc};

use super::super::cache::{CacheSource, CachedPath};
use super::super::capture::Capture;
use super::{Resolved, Source};

fn shell() -> PathBuf {
    PathBuf::from("/fixture/bin/zsh")
}

/// A `PATH` with a marked entry, and network variables that hold fake
/// credentials, built at run time.
fn resolved() -> Resolved {
    Resolved {
        path: std::env::join_paths(["/fixture/secret/bin", "/usr/bin", "/opt/homebrew/bin"])
            .unwrap(),
        source: Source::LoginShell(shell()),
        capture: Capture::Captured {
            shell: shell(),
            elapsed_ms: 3_449,
        },
        added: 2,
        fallback: vec![PathBuf::from("/opt/homebrew/bin")],
        cached: None,
        changed: true,
        network: vec![
            (
                "HTTPS_PROXY".to_string(),
                format!("http://{}:{}@proxy.fixture.invalid:8080", "user", "pw"),
            ),
            ("GITHUB_TOKEN".to_string(), format!("ghp_{}", "fixture0000")),
        ],
    }
}

#[test]
fn log_fields_hold_no_path_entry_or_environment_value() {
    let now = Utc.with_ymd_and_hms(2026, 11, 9, 12, 0, 0).unwrap();
    let fields = resolved().log_fields(now);
    assert_eq!(
        fields,
        [
            ("source", "login shell /fixture/bin/zsh".to_string()),
            ("capture", "captured".to_string()),
            ("capture_seconds", "3.4".to_string()),
            ("path_from", "login_shell".to_string()),
            ("cache_age", "-".to_string()),
            ("added", "2".to_string()),
            ("fallback", "/opt/homebrew/bin".to_string()),
            ("changed", "true".to_string()),
            ("network_variables", "2".to_string()),
        ]
    );
    let failed = Resolved {
        capture: Capture::Timeout {
            shell: shell(),
            limit_ms: 5_000,
        },
        cached: Some(CachedPath {
            saved_at: now - chrono::Duration::days(3),
            source: CacheSource::Terminal,
            version: "1".into(),
            added: 1,
        }),
        ..resolved()
    };
    let fields = failed.log_fields(now);
    assert!(fields.contains(&("capture_seconds", "5.0".to_string())));
    assert!(fields.contains(&("path_from", "cache".to_string())));
    assert!(fields.contains(&("cache_age", "259200".to_string())));
    for (name, value) in &fields {
        for secret in [
            "/fixture/secret/bin",
            "proxy.fixture.invalid",
            "user:pw",
            "HTTPS_PROXY",
            "GITHUB_TOKEN",
            "ghp_",
        ] {
            assert!(!value.contains(secret), "{name} holds {secret}");
        }
    }
}
