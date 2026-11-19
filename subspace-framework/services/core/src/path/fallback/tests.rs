use std::path::{Path, PathBuf};

use super::{
    LINUX_DIRECTORIES, MACOS_DIRECTORIES, Probe, WINDOWS_DIRECTORIES, expand_all,
    missing_directories, parse_fixture,
};

fn every_directory(_: &Path) -> bool {
    true
}

fn windows_variable(name: &str) -> Option<String> {
    match name {
        "ProgramFiles" => Some(r"C:\Fixture Programs".into()),
        "ProgramData" => Some(r"C:\FixtureData".into()),
        "USERPROFILE" => Some(r"C:\Users\fixture".into()),
        "ProgramFiles(x86)" => Some(String::new()),
        _ => None,
    }
}

fn no_variable(_: &str) -> Option<String> {
    None
}

fn probe(home: Option<&str>, lookup: &'static dyn Fn(&str) -> Option<String>) -> Probe<'static> {
    Probe {
        home: home.map(PathBuf::from),
        lookup,
        is_directory: every_directory,
        windows: false,
    }
}

fn paths(entries: &[&str]) -> Vec<PathBuf> {
    entries.iter().map(PathBuf::from).collect()
}

#[test]
fn the_macos_list_puts_system_folders_before_the_home_folder() {
    assert_eq!(
        expand_all(
            MACOS_DIRECTORIES,
            &probe(Some("/fixture/home"), &no_variable)
        ),
        paths(&[
            "/opt/homebrew/bin",
            "/opt/homebrew/sbin",
            "/usr/local/bin",
            "/usr/local/sbin",
            "/opt/local/bin",
            "/fixture/home/bin",
            "/fixture/home/.local/bin",
        ])
    );
}

#[test]
fn the_linux_list_skips_home_folders_without_a_home() {
    assert_eq!(
        expand_all(
            LINUX_DIRECTORIES,
            &probe(Some("/fixture/home"), &no_variable)
        ),
        paths(&[
            "/usr/local/bin",
            "/snap/bin",
            "/home/linuxbrew/.linuxbrew/bin",
            "/fixture/home/.linuxbrew/bin",
            "/fixture/home/bin",
            "/fixture/home/.local/bin",
        ])
    );
    assert_eq!(
        expand_all(LINUX_DIRECTORIES, &probe(None, &no_variable)),
        paths(&[
            "/usr/local/bin",
            "/snap/bin",
            "/home/linuxbrew/.linuxbrew/bin"
        ])
    );
}

#[test]
fn the_windows_list_reads_variables_and_skips_unset_or_empty_ones() {
    assert_eq!(
        expand_all(WINDOWS_DIRECTORIES, &probe(None, &windows_variable)),
        paths(&[
            r"C:\Fixture Programs\Microsoft SDKs\Azure\CLI2\wbin",
            r"C:\FixtureData\chocolatey\bin",
            r"C:\Users\fixture\scoop\shims",
            r"C:\Users\fixture\.local\bin",
        ])
    );
    assert!(expand_all(WINDOWS_DIRECTORIES, &probe(None, &no_variable)).is_empty());
}

#[test]
fn the_debug_fixture_is_a_path_list_and_empty_means_none() {
    let first = std::env::temp_dir().join("fixture-a");
    let second = std::env::temp_dir().join("fixture-b");
    let value = std::env::join_paths([&first, &second]).unwrap();
    assert_eq!(parse_fixture(&value), [first.clone(), second]);
    assert!(parse_fixture(std::ffi::OsStr::new("")).is_empty());
    let with_gap = std::env::join_paths([first.as_os_str(), std::ffi::OsStr::new("")]).unwrap();
    assert_eq!(parse_fixture(&with_gap), [first]);
}

#[test]
fn only_existing_folders_missing_from_path_are_appended_once() {
    let directory = tempfile::tempdir().unwrap();
    let present = directory.path().join("present");
    let existing = directory.path().join("existing");
    std::fs::create_dir(&present).unwrap();
    std::fs::create_dir(&existing).unwrap();
    let candidates = vec![
        present.join(""),
        directory.path().join("absent"),
        existing.clone(),
        existing.clone(),
    ];
    let probe = Probe {
        home: None,
        lookup: &no_variable,
        is_directory: Path::is_dir,
        windows: false,
    };
    assert_eq!(
        missing_directories(&[present], candidates, &probe),
        [existing]
    );
}

#[test]
fn windows_folders_compare_without_case_or_separator_differences() {
    let probe = Probe {
        home: None,
        lookup: &no_variable,
        is_directory: every_directory,
        windows: true,
    };
    assert!(
        missing_directories(
            &[PathBuf::from("c:/tools/")],
            vec![PathBuf::from(r"C:\Tools")],
            &probe
        )
        .is_empty()
    );
    assert_eq!(
        missing_directories(
            &[PathBuf::from("c:/tools/")],
            vec![PathBuf::from(r"C:\Other")],
            &probe
        ),
        [PathBuf::from(r"C:\Other")]
    );
}
