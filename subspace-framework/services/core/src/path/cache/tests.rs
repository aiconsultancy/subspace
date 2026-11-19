use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{TimeZone as _, Utc};

use super::{
    CacheSource, CachedTool, PathCache, cached_tool_in, read_cache, remember_in, write_cache,
};

/// An absolute fixture path on every platform; nothing is created there.
fn fixture(relative: &str) -> PathBuf {
    std::env::temp_dir()
        .join("subspace-path-fixture")
        .join(relative)
}

fn cache() -> PathCache {
    PathCache {
        version: "1".into(),
        saved_at: Utc.with_ymd_and_hms(2026, 11, 6, 12, 0, 0).unwrap(),
        source: CacheSource::Terminal,
        path: vec![PathBuf::from("/usr/bin")],
        tools: BTreeMap::from([("az".to_string(), fixture("bin/az"))]),
    }
}

#[test]
fn relative_tool_paths_in_the_file_are_dropped() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("path-cache.json");
    let mut with_relative = cache();
    with_relative
        .tools
        .insert("kubectl".into(), PathBuf::from("bin/kubectl"));
    write_cache(&file, &with_relative).unwrap();
    let read = read_cache(&file).unwrap();
    assert_eq!(read.tools.get("kubectl"), None);
    assert_eq!(read.tools.get("az"), Some(&fixture("bin/az")));
}

#[test]
fn a_registry_source_round_trips() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("path-cache.json");
    let registry = PathCache {
        source: CacheSource::Registry,
        ..cache()
    };
    write_cache(&file, &registry).unwrap();
    assert_eq!(read_cache(&file), Some(registry));
}

#[test]
fn a_corrupt_foreign_or_mistyped_file_reads_as_absent_and_a_good_save_replaces_it() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("path-cache.json");
    assert_eq!(read_cache(&file), None, "missing");
    for text in [
        "not json",
        r#"{"version":"2","saved_at":"2026-11-06T12:00:00Z","source":{"kind":"terminal"},"path":[]}"#,
        r#"{"version":"1","saved_at":"yesterday","source":{"kind":"terminal"},"path":[]}"#,
        r#"{"version":"1","saved_at":"2026-11-06T12:00:00Z","source":{"kind":"robot"},"path":[]}"#,
        r#"{"version":"1","saved_at":"2026-11-06T12:00:00Z","source":{"kind":"terminal"},"path":"/usr/bin"}"#,
    ] {
        std::fs::write(&file, text).unwrap();
        assert_eq!(read_cache(&file), None, "{text}");
    }
    write_cache(&file, &cache()).unwrap();
    assert_eq!(read_cache(&file), Some(cache()));
}

#[test]
fn a_folder_in_place_of_the_file_reads_as_absent() {
    let directory = tempfile::tempdir().unwrap();
    assert_eq!(read_cache(directory.path()), None);
}

#[cfg(unix)]
#[test]
fn the_file_is_private_to_its_owner() {
    use std::os::unix::fs::PermissionsExt as _;

    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("path-cache.json");
    write_cache(&file, &cache()).unwrap();
    let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn a_tool_path_is_kept_as_found_and_saved_into_an_existing_cache_only() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("path-cache.json");
    assert!(remember_in(&file, "kubectl", &fixture("bin/kubectl")).is_err());
    assert_eq!(read_cache(&file), None);
    write_cache(&file, &cache()).unwrap();
    remember_in(&file, "kubectl", &fixture("link/kubectl")).unwrap();
    let saved = read_cache(&file).unwrap();
    assert_eq!(saved.tools.get("kubectl"), Some(&fixture("link/kubectl")));
    assert_eq!(saved.tools.get("az"), Some(&fixture("bin/az")));
    assert_eq!(saved.saved_at, cache().saved_at);
}

#[test]
fn a_saved_tool_reports_whether_it_can_still_run() {
    let saved = cache();
    assert_eq!(
        cached_tool_in(&saved, "az", |_| false),
        Some(CachedTool {
            path: fixture("bin/az"),
            saved_at: saved.saved_at,
            usable: false,
        })
    );
    assert!(cached_tool_in(&saved, "az", |_| true).unwrap().usable);
    assert_eq!(cached_tool_in(&saved, "gh", |_| true), None);
}

#[cfg(unix)]
#[test]
fn readonly_cache_lookup_validates_without_recording_or_writing() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("path-cache.json");
    let tool = directory.path().join("readonly-fixture-tool");
    crate::executable_tests::write_executable(&tool, "#!/bin/sh\n");
    let name = "readonly-fixture-tool";
    let mut saved = cache();
    saved.tools.insert(name.into(), tool.clone());
    write_cache(&file, &saved).unwrap();
    let before = std::fs::read(&file).unwrap();
    let logs_before = crate::log::recent()
        .into_iter()
        .filter(|entry| entry.message == "cached tool path used" && entry.line().contains(name))
        .count();
    assert_eq!(
        super::cached_tool_readonly_in(&file, name),
        Some(tool.clone())
    );
    assert_eq!(std::fs::read(&file).unwrap(), before);
    let logs_after = crate::log::recent()
        .into_iter()
        .filter(|entry| entry.message == "cached tool path used" && entry.line().contains(name))
        .count();
    assert_eq!(logs_after, logs_before);
    let observed = super::read_cached_tool(&file, name).unwrap();
    assert_eq!(super::record_tool_use(name, observed), Some(tool.clone()));
    assert!(
        crate::log::recent()
            .into_iter()
            .any(|entry| entry.message == "cached tool path used" && entry.line().contains(name))
    );
    std::fs::remove_file(tool).unwrap();
    assert_eq!(super::cached_tool_readonly_in(&file, name), None);
}
