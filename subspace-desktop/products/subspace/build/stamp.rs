//! The build stamp: the `cargo:` directives that set the `SUBSPACE_BUILD_*`
//! variables the app's `BuildInfo` reads (commit, dirty flag, branch, time,
//! target, profile, kind and host) and re-run the script when this
//! workspace's sources or the checked-out commit change. `build.rs` prints
//! them; the app's tests include this file to test it.

use std::path::{Path, PathBuf};
use std::process::Command;

/// What the build script reads from Cargo and the machine.
pub(crate) struct BuildEnv {
    /// This workspace's root.
    pub(crate) workspace: PathBuf,
    /// A pre-release build names its commit here, because stamping the
    /// version leaves the checkout modified.
    pub(crate) source_commit: Option<String>,
    pub(crate) target: String,
    pub(crate) profile: String,
    /// Whether the `capture` feature is on.
    pub(crate) capture: bool,
    pub(crate) machine: String,
    pub(crate) unix_time: u64,
}

/// Every `cargo:` directive of the stamp, in order.
pub(crate) fn directives(env: &BuildEnv) -> Vec<String> {
    let branch = git_branch(&env.workspace).unwrap_or_else(|| "unknown".into());
    let mut lines = commit_directives(env);
    lines.extend(environment_directives(env, &branch));
    lines.extend(watch_directives(&env.workspace, &branch));
    lines
}

fn commit_directives(env: &BuildEnv) -> Vec<String> {
    let (commit, dirty) = match env.source_commit.as_deref() {
        Some(commit) if !commit.is_empty() => (commit.to_owned(), false),
        _ => (
            git_commit_hash(&env.workspace).unwrap_or_else(|| "unknown".into()),
            git_dirty(&env.workspace),
        ),
    };
    vec![
        "cargo:rerun-if-env-changed=SUBSPACE_SOURCE_COMMIT".to_owned(),
        format!("cargo:rustc-env=SUBSPACE_BUILD_COMMIT={commit}"),
        format!("cargo:rustc-env=SUBSPACE_BUILD_DIRTY={dirty}"),
    ]
}

fn environment_directives(env: &BuildEnv, branch: &str) -> Vec<String> {
    [
        ("BRANCH", branch.to_owned()),
        ("TARGET", env.target.clone()),
        ("PROFILE", env.profile.clone()),
        ("TIMESTAMP", timestamp::utc(env.unix_time)),
        ("UNIX", env.unix_time.to_string()),
        ("KIND", build_kind(&env.profile, env.capture).to_owned()),
        ("HOST", build_host(env.capture, &env.machine)),
    ]
    .into_iter()
    .map(|(name, value)| format!("cargo:rustc-env=SUBSPACE_BUILD_{name}={value}"))
    .collect()
}

/// Re-runs when this workspace's sources or the checked-out commit change.
fn watch_directives(workspace: &Path, branch: &str) -> Vec<String> {
    let sources = [
        "Cargo.toml",
        "Cargo.lock",
        "spaces",
        "clients",
        "products",
        "terminal",
    ]
    .into_iter()
    .map(|path| workspace.join(path));
    let refs = [
        "HEAD".to_owned(),
        format!("refs/heads/{branch}"),
        "packed-refs".to_owned(),
    ]
    .into_iter()
    .filter_map(|name| git_file(workspace, &name));
    sources
        .chain(refs)
        .map(|path| format!("cargo:rerun-if-changed={}", path.display()))
        .collect()
}

/// `release` for release builds, `test` for capture builds and `dev` for
/// other local builds; the shown version marks the last two.
fn build_kind(profile: &str, capture: bool) -> &'static str {
    if profile == "release" {
        "release"
    } else if capture {
        "test"
    } else {
        "dev"
    }
}

/// The host part of a dev or test version: the machine's short name as a
/// semver identifier. Capture builds use `capture`, so no real host name
/// reaches screenshots.
fn build_host(capture: bool, machine: &str) -> String {
    if capture {
        return "capture".into();
    }
    let short = machine.trim().split('.').next().unwrap_or_default();
    let host: String = short
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if host.is_empty() {
        "unknown".into()
    } else {
        host
    }
}

/// The machine's host name, or nothing when `hostname` fails.
pub(crate) fn machine_name() -> String {
    Command::new("hostname")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .unwrap_or_default()
}

fn git_branch(repository: &Path) -> Option<String> {
    git_output(repository, &["symbolic-ref", "--short", "HEAD"])
        .or_else(|| git_output(repository, &["rev-parse", "--short", "HEAD"]))
}

fn git_commit_hash(repository: &Path) -> Option<String> {
    git_output(repository, &["rev-parse", "--short", "HEAD"])
}

fn git_dirty(repository: &Path) -> bool {
    Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=no"])
        .current_dir(repository)
        .output()
        .ok()
        .is_some_and(|output| output.status.success() && !output.stdout.is_empty())
}

fn git_output(repository: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(repository)
        .output()
        .ok()?;
    let value = String::from_utf8(output.stdout).ok()?;
    output.status.success().then(|| value.trim().to_owned())
}

/// The git file `name` resolves to, such as `.git/HEAD`, for a re-run
/// directive.
fn git_file(repository: &Path, name: &str) -> Option<PathBuf> {
    let path = PathBuf::from(git_output(repository, &["rev-parse", "--git-path", name])?);
    Some(if path.is_absolute() {
        path
    } else {
        repository.join(path)
    })
}

pub(crate) mod timestamp {
    use std::time::{SystemTime, UNIX_EPOCH};

    const SECONDS_PER_DAY: u64 = 86_400;
    const SECONDS_PER_HOUR: u64 = 3_600;
    const SECONDS_PER_MINUTE: u64 = 60;
    const CIVIL_EPOCH_OFFSET: i64 = 719_468;
    const ERA_NEGATIVE_ADJUSTMENT: i64 = 146_096;
    const DAYS_PER_ERA: i64 = 146_097;
    const DAYS_PER_FOUR_YEARS: i64 = 1_460;
    const DAYS_PER_CENTURY: i64 = 36_524;
    const DAYS_PER_YEAR: i64 = 365;
    const YEARS_PER_ERA: i64 = 400;
    const LEAP_YEAR_INTERVAL: i64 = 4;
    const YEARS_PER_CENTURY: i64 = 100;
    const MONTH_SCALE: i64 = 5;
    const MONTH_DIVISOR: i64 = 153;
    const MARCH_BASED_MONTHS: i64 = 10;
    const MARCH_OFFSET: i64 = 3;
    const DECEMBER_OFFSET: i64 = -9;

    pub(crate) fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs())
    }

    pub(crate) fn utc(seconds: u64) -> String {
        let (year, month, day) = civil_date((seconds / SECONDS_PER_DAY) as i64);
        let day_seconds = seconds % SECONDS_PER_DAY;
        let hour = day_seconds / SECONDS_PER_HOUR;
        let minute = day_seconds % SECONDS_PER_HOUR / SECONDS_PER_MINUTE;
        let second = day_seconds % SECONDS_PER_MINUTE;
        format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
    }

    fn civil_date(days_since_epoch: i64) -> (i64, i64, i64) {
        let days = days_since_epoch + CIVIL_EPOCH_OFFSET;
        let era = (if days >= 0 {
            days
        } else {
            days - ERA_NEGATIVE_ADJUSTMENT
        }) / DAYS_PER_ERA;
        let day_of_era = days - era * DAYS_PER_ERA;
        let year_of_era = (day_of_era - day_of_era / DAYS_PER_FOUR_YEARS
            + day_of_era / DAYS_PER_CENTURY
            - day_of_era / ERA_NEGATIVE_ADJUSTMENT)
            / DAYS_PER_YEAR;
        let mut year = year_of_era + era * YEARS_PER_ERA;
        let day_of_year = day_of_era
            - (DAYS_PER_YEAR * year_of_era + year_of_era / LEAP_YEAR_INTERVAL
                - year_of_era / YEARS_PER_CENTURY);
        let month_prime = (MONTH_SCALE * day_of_year + 2) / MONTH_DIVISOR;
        let day = day_of_year - (MONTH_DIVISOR * month_prime + 2) / MONTH_SCALE + 1;
        let month = month_prime
            + if month_prime < MARCH_BASED_MONTHS {
                MARCH_OFFSET
            } else {
                DECEMBER_OFFSET
            };
        year += i64::from(month <= 2);
        (year, month, day)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::process::Command;

    use super::{BuildEnv, build_host, build_kind, directives, machine_name, timestamp};

    /// Re-run only this test with Git's hook-local environment removed. Changing
    /// the parent's environment would race with the other test threads.
    fn rerun_without_git_environment() -> bool {
        let thread = std::thread::current();
        let name = thread.name().unwrap();
        if std::env::var("SUBSPACE_STAMP_TEST").as_deref() == Ok(name) {
            return false;
        }
        let local = Command::new("git")
            .args(["rev-parse", "--local-env-vars"])
            .output()
            .unwrap();
        assert!(local.status.success());
        let mut child = Command::new(std::env::current_exe().unwrap());
        child.args(["--exact", name, "--nocapture"]);
        child.env("SUBSPACE_STAMP_TEST", name);
        for variable in String::from_utf8(local.stdout).unwrap().lines() {
            child.env_remove(variable);
        }
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "{name}:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        true
    }

    fn env(workspace: &Path, source_commit: Option<&str>, capture: bool) -> BuildEnv {
        BuildEnv {
            workspace: workspace.to_owned(),
            source_commit: source_commit.map(str::to_owned),
            target: "aarch64-apple-darwin".into(),
            profile: "debug".into(),
            capture,
            machine: "studio.local".into(),
            unix_time: 1_763_290_000,
        }
    }

    #[test]
    fn only_release_builds_are_release_and_capture_builds_are_tests() {
        assert_eq!(build_kind("release", true), "release");
        assert_eq!(build_kind("debug", true), "test");
        assert_eq!(build_kind("debug", false), "dev");
    }

    #[test]
    fn the_host_is_a_semver_safe_short_name_and_hidden_in_captures() {
        assert_eq!(build_host(false, "Zack's Mac.local\n"), "Zack-s-Mac");
        assert_eq!(build_host(false, ""), "unknown");
        assert_eq!(build_host(true, "studio"), "capture");
        assert!(!build_host(false, &machine_name()).is_empty());
    }

    #[test]
    fn utc_dates_cross_leap_days_and_eras() {
        assert_eq!(timestamp::utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(timestamp::utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(timestamp::utc(1_763_290_000), "2025-11-16T10:46:40Z");
        assert!(timestamp::now() > 1_763_290_000);
    }

    #[test]
    fn outside_a_checkout_the_stamp_names_unknown_and_watches_the_sources() {
        if rerun_without_git_environment() {
            return;
        }
        let folder = tempfile::tempdir().unwrap();
        let lines = directives(&env(folder.path(), None, false));
        for expected in [
            "cargo:rustc-env=SUBSPACE_BUILD_COMMIT=unknown",
            "cargo:rustc-env=SUBSPACE_BUILD_DIRTY=false",
            "cargo:rustc-env=SUBSPACE_BUILD_BRANCH=unknown",
            "cargo:rustc-env=SUBSPACE_BUILD_KIND=dev",
            "cargo:rustc-env=SUBSPACE_BUILD_HOST=studio",
            "cargo:rustc-env=SUBSPACE_BUILD_UNIX=1763290000",
            "cargo:rustc-env=SUBSPACE_BUILD_TIMESTAMP=2025-11-16T10:46:40Z",
            "cargo:rustc-env=SUBSPACE_BUILD_TARGET=aarch64-apple-darwin",
            "cargo:rustc-env=SUBSPACE_BUILD_PROFILE=debug",
        ] {
            assert!(
                lines.iter().any(|line| line == expected),
                "{expected}: {lines:?}"
            );
        }
        let watched = format!(
            "cargo:rerun-if-changed={}",
            folder.path().join("spaces").display()
        );
        assert!(lines.contains(&watched));
    }

    #[test]
    fn a_named_source_commit_wins_and_is_never_dirty() {
        if rerun_without_git_environment() {
            return;
        }
        let folder = tempfile::tempdir().unwrap();
        let lines = directives(&env(folder.path(), Some("abc1234"), true));
        assert!(lines.contains(&"cargo:rustc-env=SUBSPACE_BUILD_COMMIT=abc1234".to_owned()));
        assert!(lines.contains(&"cargo:rustc-env=SUBSPACE_BUILD_DIRTY=false".to_owned()));
        assert!(lines.contains(&"cargo:rustc-env=SUBSPACE_BUILD_KIND=test".to_owned()));
        assert!(lines.contains(&"cargo:rustc-env=SUBSPACE_BUILD_HOST=capture".to_owned()));
    }

    #[test]
    fn inside_this_checkout_the_stamp_names_a_commit_and_watches_head() {
        if rerun_without_git_environment() {
            return;
        }
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let lines = directives(&env(&workspace, None, false));
        let commit = lines
            .iter()
            .find_map(|line| line.strip_prefix("cargo:rustc-env=SUBSPACE_BUILD_COMMIT="))
            .unwrap();
        // A source archive without git stamps `unknown`, like a folder.
        if commit != "unknown" {
            assert!(commit.chars().all(|c| c.is_ascii_hexdigit()), "{commit}");
            assert!(lines.iter().any(|line| line.ends_with("HEAD")), "{lines:?}");
        }
    }
}
