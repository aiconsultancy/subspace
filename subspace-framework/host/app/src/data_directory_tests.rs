//! A fictional app whose binary, `harbor`, differs from its data name keeps
//! every file in one data directory. The data name and channel are
//! process-wide, so each case runs in a child process of its own. Unix only:
//! the cases isolate the platform roots through `HOME`, which Windows does
//! not use for them.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;

use subspace_core::credentials::signed_in::keychain_service;
use subspace_core::{config, log, state};
use subspace_product::{
    Channel, DEFAULT_THEME, PlatformEnv, ProductContext, ProductDescriptor, configure_channel,
    configure_data_name, fixture, validate_product,
};

use crate::product::ProductFiles;
use crate::startup::name_data_directory;
use crate::{prepare_context, record_new_install_theme};

const CASE: &str = "SUBSPACE_DATA_DIRECTORY_CASE";
const CHILD: &str = "data_directory_tests::one_case_in_a_child_process";

fn harbor_descriptor() -> ProductDescriptor {
    let mut descriptor = fixture::descriptor();
    descriptor.binary_name = "harbor".into();
    descriptor
}

fn harbor() -> ProductContext {
    let product = validate_product(&harbor_descriptor()).unwrap();
    ProductContext::new(product, PlatformEnv::from_process().unwrap())
}

/// Every file and folder under the child's `HOME`, relative to it.
fn written() -> Vec<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let mut pending = vec![home.clone()];
    let mut found = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path.clone());
            }
            found.push(path.strip_prefix(&home).unwrap().to_owned());
        }
    }
    found
}

/// Every platform root of the child derives from its temporary `HOME`, so
/// a case may write.
fn assert_isolated() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    assert!(home.starts_with(std::env::temp_dir()), "{home:?}");
    let env = PlatformEnv::from_process().unwrap();
    assert_eq!(env.home, home);
    let overrides = [
        env.xdg_config_home,
        env.xdg_state_home,
        env.app_data,
        env.local_app_data,
    ];
    assert_eq!(overrides, [None, None, None, None]);
}

/// Records a new install's theme for the `harbor` context `context` gives:
/// everything lands under `harbor`, nothing under `subspace`.
fn records_only_under_harbor(context: impl FnOnce() -> ProductContext) {
    assert_isolated();
    let context = context();
    assert_eq!(
        record_new_install_theme(&context, DEFAULT_THEME, &[]),
        Ok(true)
    );
    let files = ProductFiles::for_context(&context);
    assert!(files.state.is_file(), "{:?}", files.state);
    directory_of(&files.state, &Channel::current().name("harbor"));
    let stray = written()
        .into_iter()
        .filter(|path| path.to_string_lossy().contains("subspace"))
        .collect::<Vec<_>>();
    assert!(stray.is_empty(), "{stray:?}");
}

/// A raw core read first: `attempt` refuses and nothing is written.
fn refuses_after_a_read(attempt: impl FnOnce() -> Result<(), String>) {
    assert_isolated();
    assert!(state::path().is_some());
    let error = attempt().unwrap_err();
    assert!(error.contains("configure_data_name"), "{error}");
    assert_eq!(written(), Vec::<PathBuf>::new());
}

fn prepare_harbor() -> Result<ProductContext, String> {
    prepare_context(harbor_descriptor(), "1.0.0")
}

fn record_for_constructed_harbor() -> Result<(), String> {
    record_new_install_theme(&harbor(), DEFAULT_THEME, &[]).map(|_| ())
}

/// The directory of `file`, which must be named `name`.
fn directory_of(file: &Path, name: &str) -> PathBuf {
    let directory = file.parent().unwrap().to_owned();
    assert_eq!(directory.file_name().unwrap(), name, "{file:?}");
    directory
}

/// Every product and core path, and the Keychain service, use `name`.
fn assert_one_directory(name: &str) {
    let files = ProductFiles::for_context(&harbor());
    let core_state = state::path().unwrap();
    let directory = directory_of(&core_state, name);
    assert_eq!(files.state, core_state);
    assert_eq!(files.config, config::path().unwrap());
    directory_of(&files.config, name);
    let env = PlatformEnv::from_process().unwrap();
    let saved_path = crate::startup_path::saved_path_file(&env);
    assert_eq!(saved_path.parent().unwrap(), directory);
    let logs = crate::logging::directory(&files.state).unwrap();
    assert_eq!(Some(&logs), log::default_directory().as_ref());
    assert_eq!(state::sibling("logs"), Some(logs));
    assert_eq!(
        state::sibling("cache.json"),
        Some(directory.join("cache.json"))
    );
    assert_eq!(keychain_service(), format!("{name}.github"));
}

fn run_case(case: &str) {
    match case {
        "named" => {
            assert!(configure_channel(Channel::Stable));
            assert_eq!(configure_data_name("example-app"), Ok(()));
            assert_eq!(name_data_directory("harbor"), Ok(()));
            assert_one_directory("example-app");
        }
        "named-beta" => {
            assert!(configure_channel(Channel::Beta));
            assert_eq!(configure_data_name("example-app"), Ok(()));
            assert_eq!(name_data_directory("harbor"), Ok(()));
            assert_one_directory("example-app-beta");
        }
        "binary" => {
            assert_eq!(name_data_directory("harbor"), Ok(()));
            assert_one_directory(&Channel::current().name("harbor"));
        }
        "read-first" => {
            let before = state::path().unwrap();
            let error = name_data_directory("harbor").unwrap_err();
            assert!(error.contains("configure_data_name"), "{error}");
            assert_eq!(state::path(), Some(before));
            assert_one_directory(&Channel::current().name("subspace"));
        }
        "prepare" => records_only_under_harbor(|| prepare_harbor().unwrap()),
        "prepare-read-first" => refuses_after_a_read(|| prepare_harbor().map(|_| ())),
        "constructed" => records_only_under_harbor(harbor),
        "constructed-read-first" => refuses_after_a_read(record_for_constructed_harbor),
        other => panic!("unknown case {other}"),
    }
}

#[test]
fn one_case_in_a_child_process() {
    let Ok(case) = std::env::var(CASE) else {
        assert!(std::env::var(CASE).is_err());
        return;
    };
    run_case(&case);
}

#[test]
fn every_path_of_an_app_shares_one_data_directory() {
    let cases = [
        "named",
        "named-beta",
        "binary",
        "read-first",
        "prepare",
        "prepare-read-first",
        "constructed",
        "constructed-read-first",
    ];
    for case in cases {
        let home = tempfile::tempdir().unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CHILD, "--nocapture"])
            .env(CASE, case)
            .env("HOME", home.path())
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("XDG_STATE_HOME")
            .env_remove("APPDATA")
            .env_remove("LOCALAPPDATA")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{case}: {stdout}{stderr}");
        assert!(stdout.contains("1 passed"), "{case}: {stdout}");
    }
}
