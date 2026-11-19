use std::cell::Cell;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{TimeZone as _, Utc};

use super::super::cache::{CacheSource, CachedPath, PathCache};
use super::super::capture::Capture;
use super::super::fallback::Probe;
use super::super::{CACHED_SHELL_TIMEOUT, SHELL_TIMEOUT};
use super::{CaptureError, Found, Inputs, Source, disabled_by, resolve_with};

fn joined(entries: &[&str]) -> OsString {
    std::env::join_paths(entries).unwrap()
}

fn fixture_directory(path: &Path) -> bool {
    path.starts_with("/fixture/exists")
}

fn no_variable(_: &str) -> Option<String> {
    None
}

const FIXED: &[&str] = &[
    "/fixture/exists/homebrew/bin",
    "/fixture/missing/homebrew/sbin",
    "/fixture/exists/usr/local/bin",
    "/fixture/exists/already/bin",
];

fn probe() -> Probe<'static> {
    Probe {
        home: None,
        lookup: &no_variable,
        is_directory: fixture_directory,
        windows: false,
    }
}

fn inputs<'a, C>(current: &[&str], capture: C, cache: Option<&'a PathCache>) -> Inputs<'a, C> {
    Inputs {
        current: joined(current),
        disabled: None,
        terminal: false,
        capture,
        cache,
        fallback: FIXED.iter().map(PathBuf::from).collect(),
        probe: probe(),
        inherited_first: false,
    }
}

fn shell() -> PathBuf {
    PathBuf::from("/fixture/bin/zsh")
}

fn found(path: &[&str]) -> Result<Found, CaptureError> {
    Ok(Found {
        path: joined(path),
        source: Source::LoginShell(shell()),
        capture: Capture::Captured {
            shell: shell(),
            elapsed_ms: 3_449,
        },
        network: Vec::new(),
    })
}

fn timed_out(_: Duration) -> Result<Found, CaptureError> {
    Err(CaptureError {
        capture: Capture::Timeout {
            shell: shell(),
            limit_ms: 5_000,
        },
        message: "login shell /fixture/bin/zsh: no PATH within 5 seconds".into(),
    })
}

fn saved_cache(path: &[&str]) -> PathCache {
    PathCache {
        version: "1".into(),
        saved_at: Utc.with_ymd_and_hms(2026, 11, 6, 12, 0, 0).unwrap(),
        source: CacheSource::LoginShell { shell: shell() },
        path: path.iter().map(PathBuf::from).collect(),
        tools: BTreeMap::from([("az".to_string(), PathBuf::from("/fixture/old/bin/az"))]),
    }
}

#[test]
fn fallback_appends_only_existing_missing_directories_in_order() {
    let resolved = resolve_with(inputs(
        &["/usr/bin", "/fixture/exists/already/bin"],
        timed_out,
        None,
    ));
    assert_eq!(
        resolved.path,
        joined(&[
            "/usr/bin",
            "/fixture/exists/already/bin",
            "/fixture/exists/homebrew/bin",
            "/fixture/exists/usr/local/bin",
        ])
    );
    assert_eq!(
        resolved.fallback,
        [
            PathBuf::from("/fixture/exists/homebrew/bin"),
            PathBuf::from("/fixture/exists/usr/local/bin"),
        ]
    );
    assert_eq!(resolved.added, 2);
    assert!(resolved.changed, "apply must set PATH for fallback entries");
    assert!(resolved.capture_failed());
    assert_eq!(resolved.path_from(), "fallback");
    assert_eq!(
        resolved.source,
        Source::Inherited("login shell /fixture/bin/zsh: no PATH within 5 seconds".into())
    );
}

#[test]
fn a_successful_login_path_keeps_its_order_and_ignores_the_cache() {
    let cache = saved_cache(&["/fixture/exists/cached/bin"]);
    let resolved = resolve_with(inputs(
        &["/usr/bin", "/bin"],
        |_| {
            found(&[
                "/login/second",
                "/login/first",
                "/fixture/exists/homebrew/bin",
            ])
        },
        Some(&cache),
    ));
    assert_eq!(
        resolved.path,
        joined(&[
            "/login/second",
            "/login/first",
            "/fixture/exists/homebrew/bin",
            "/usr/bin",
            "/bin",
            "/fixture/exists/usr/local/bin",
            "/fixture/exists/already/bin",
        ])
    );
    assert_eq!(resolved.added, 5);
    assert_eq!(resolved.cached, None);
    assert_eq!(resolved.path_from(), "login_shell");
}

#[test]
fn a_registry_capture_keeps_the_inherited_path_first() {
    let mut inputs = inputs(
        &["/usr/bin", "/bin"],
        |_| {
            Ok(Found {
                path: joined(&["/registry/machine", "/usr/bin", "/registry/user"]),
                source: Source::Registry,
                capture: Capture::Registry,
                network: Vec::new(),
            })
        },
        None,
    );
    inputs.inherited_first = true;
    let resolved = resolve_with(inputs);
    assert_eq!(
        resolved.path,
        joined(&[
            "/usr/bin",
            "/bin",
            "/registry/machine",
            "/registry/user",
            "/fixture/exists/homebrew/bin",
            "/fixture/exists/usr/local/bin",
            "/fixture/exists/already/bin",
        ])
    );
    assert_eq!(resolved.capture, Capture::Registry);
    assert_eq!(resolved.path_from(), "registry");
}

#[test]
fn a_failed_shell_adds_existing_saved_folders_before_the_fixed_ones() {
    let cache = saved_cache(&[
        "/usr/bin",
        "/fixture/exists/cached/bin",
        "/fixture/missing/stale/bin",
        "/fixture/exists/usr/local/bin",
    ]);
    let resolved = resolve_with(inputs(&["/usr/bin", "/bin"], timed_out, Some(&cache)));
    assert_eq!(
        resolved.path,
        joined(&[
            "/usr/bin",
            "/bin",
            "/fixture/exists/cached/bin",
            "/fixture/exists/usr/local/bin",
            "/fixture/exists/homebrew/bin",
            "/fixture/exists/already/bin",
        ])
    );
    assert_eq!(
        resolved.cached,
        Some(CachedPath {
            saved_at: cache.saved_at,
            source: cache.source.clone(),
            version: "1".into(),
            added: 2,
        })
    );
    assert_eq!(
        resolved.fallback,
        [
            PathBuf::from("/fixture/exists/homebrew/bin"),
            PathBuf::from("/fixture/exists/already/bin"),
        ]
    );
    assert_eq!(resolved.added, 4);
    assert_eq!(resolved.path_from(), "cache");
}

#[test]
fn the_capture_gets_five_seconds_with_a_saved_path_and_ten_without() {
    let cache = saved_cache(&[]);
    for (present, expected) in [(true, CACHED_SHELL_TIMEOUT), (false, SHELL_TIMEOUT)] {
        let seen = Cell::new(Duration::ZERO);
        let _ = resolve_with(inputs(
            &["/usr/bin"],
            |timeout| {
                seen.set(timeout);
                timed_out(timeout)
            },
            present.then_some(&cache),
        ));
        assert_eq!(seen.get(), expected);
    }
}

#[test]
fn disabled_or_terminal_resolution_keeps_the_inherited_path() {
    let cache = saved_cache(&["/fixture/exists/cached/bin"]);
    for (disabled, terminal, capture, reason) in [
        (
            Some("SUBSPACE_GUI_LOGIN_PATH"),
            false,
            Capture::Disabled,
            "SUBSPACE_GUI_LOGIN_PATH=0",
        ),
        (
            Some("SUBSPACE_LOGIN_PATH"),
            false,
            Capture::Disabled,
            "SUBSPACE_LOGIN_PATH=0",
        ),
        (None, true, Capture::Terminal, "started from a terminal"),
    ] {
        let mut inputs = inputs(
            &["/usr/bin", "/bin"],
            |_| -> Result<Found, CaptureError> { panic!("a skipped resolution must not capture") },
            Some(&cache),
        );
        inputs.disabled = disabled;
        inputs.terminal = terminal;
        let resolved = resolve_with(inputs);
        assert_eq!(resolved.path, joined(&["/usr/bin", "/bin"]));
        assert!(resolved.fallback.is_empty());
        assert_eq!(resolved.cached, None);
        assert!(!resolved.changed);
        assert_eq!(resolved.capture, capture);
        assert_eq!(resolved.source, Source::Inherited(reason.into()));
    }
}

#[test]
fn either_variable_set_to_zero_disables_the_capture() {
    let only = |set: &'static str| move |name: &str| (name == set).then(|| "0".to_string());
    assert_eq!(
        disabled_by(&only("SUBSPACE_GUI_LOGIN_PATH")),
        Some("SUBSPACE_GUI_LOGIN_PATH")
    );
    assert_eq!(
        disabled_by(&only("SUBSPACE_LOGIN_PATH")),
        Some("SUBSPACE_LOGIN_PATH")
    );
    assert_eq!(disabled_by(&|_| Some("1".to_string())), None);
    assert_eq!(disabled_by(&no_variable), None);
}
