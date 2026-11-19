use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, TimeZone as _, Utc};

use super::super::cache::{CacheSource, PathCache, read_cache, write_cache};
use super::super::capture::Capture;
use super::super::resolve::{Resolved, Source};
use super::cache_to_save;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 9, 12, 0, 0).unwrap()
}

fn shell() -> PathBuf {
    PathBuf::from("/fixture/bin/zsh")
}

/// A captured `PATH` with a fallback folder and network variables that
/// hold fake credentials, built at run time.
fn captured() -> Resolved {
    Resolved {
        path: std::env::join_paths(["/fixture/login/bin", "/usr/bin", "/opt/homebrew/bin"])
            .unwrap(),
        source: Source::LoginShell(shell()),
        capture: Capture::Captured {
            shell: shell(),
            elapsed_ms: 900,
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

fn previous() -> PathCache {
    PathCache {
        version: "1".into(),
        saved_at: Utc.with_ymd_and_hms(2026, 11, 6, 12, 0, 0).unwrap(),
        source: CacheSource::Terminal,
        path: vec![PathBuf::from("/old/bin")],
        tools: BTreeMap::from([("az".to_string(), PathBuf::from("/fixture/old/bin/az"))]),
    }
}

#[test]
fn a_captured_path_is_saved_without_fallback_folders_and_keeps_tools() {
    let saved = cache_to_save(&captured(), Some(&previous()), now()).unwrap();
    assert_eq!(saved.source, CacheSource::LoginShell { shell: shell() });
    assert_eq!(
        saved.path,
        [
            PathBuf::from("/fixture/login/bin"),
            PathBuf::from("/usr/bin")
        ]
    );
    assert_eq!(saved.tools, previous().tools);
    assert_eq!(saved.saved_at, now());
    assert_eq!(saved.version, "1");
}

#[test]
fn a_terminal_start_is_saved_with_source_terminal() {
    let terminal = Resolved {
        capture: Capture::Terminal,
        fallback: Vec::new(),
        ..captured()
    };
    let saved = cache_to_save(&terminal, None, now()).unwrap();
    assert_eq!(saved.source, CacheSource::Terminal);
    assert!(saved.tools.is_empty());
}

#[test]
fn a_registry_read_is_saved_with_source_registry() {
    let registry = Resolved {
        source: Source::Registry,
        capture: Capture::Registry,
        ..captured()
    };
    let saved = cache_to_save(&registry, Some(&previous()), now()).unwrap();
    assert_eq!(saved.source, CacheSource::Registry);
    assert_eq!(saved.tools, previous().tools);
}

#[test]
fn disabled_and_failed_captures_are_not_saved() {
    for capture in [
        Capture::Disabled,
        Capture::Timeout {
            shell: shell(),
            limit_ms: 10_000,
        },
        Capture::NoPath {
            shell: shell(),
            elapsed_ms: 1,
        },
        Capture::StartFailed {
            shell: shell(),
            error: "entity not found".into(),
        },
        Capture::Other {
            reason: "fixture".into(),
        },
    ] {
        let resolved = Resolved {
            capture,
            ..captured()
        };
        assert_eq!(cache_to_save(&resolved, Some(&previous()), now()), None);
    }
}

#[test]
fn the_saved_file_holds_no_environment_name_or_value() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("state").join("path-cache.json");
    let resolved = captured();
    let saved = cache_to_save(&resolved, None, now()).unwrap();
    write_cache(&file, &saved).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    for (name, value) in &resolved.network {
        assert!(!text.contains(name.as_str()), "{text}");
        assert!(!text.contains(value.as_str()), "{text}");
    }
    assert!(!text.contains("proxy.fixture.invalid"), "{text}");
    assert!(!text.contains("ghp_"), "{text}");
    assert_eq!(read_cache(&file), Some(saved));
}
