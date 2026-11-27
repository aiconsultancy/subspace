//! Groups processes into apps by resident memory.
//!
//! Adapted from sysmon (https://github.com/huacnlee/sysmon,
//! `ext/sysmon-native/src/apps.rs` at 8defd3c), Copyright (c) 2026 Jason Lee,
//! MIT licence; see `NOTICE` and `LICENSE-SYSMON`. Changes: no CPU column, a
//! top-N limit, and names scrubbed of host and user names.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{BuildHasher, RandomState};
use std::sync::OnceLock;

use serde::Serialize;

use crate::privacy::Secrets;

/// How many apps a reading lists.
pub const APP_LIMIT: usize = 10;

const EXE: &str = ".exe";
const BUNDLE: &str = ".app";
/// How far up its parents a helper looks for the app that owns it.
const MAX_HELPER_DEPTH: usize = 16;

/// The `subspace.system.apps` reading.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Apps {
    /// Milliseconds since the Unix epoch.
    pub at: u64,
    /// The top apps by resident memory, largest first.
    pub apps: Vec<AppUsage>,
}

/// One app and the processes grouped into it.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct AppUsage {
    /// Opaque identity, stable for the app session. Holds no path.
    pub id: String,
    pub name: String,
    /// Summed resident memory in bytes; shared pages count more than once.
    pub memory: u64,
    pub processes: usize,
}

/// What the grouping reads from one process. Never serialised.
#[derive(Clone, Debug)]
pub(crate) struct ProcessUsage {
    pub(crate) pid: u32,
    pub(crate) parent: Option<u32>,
    pub(crate) name: String,
    pub(crate) executable: Option<String>,
    pub(crate) memory: u64,
}

pub(crate) fn observe(system: &sysinfo::System) -> Vec<ProcessUsage> {
    system
        .processes()
        .values()
        .map(|process| ProcessUsage {
            pid: process.pid().as_u32(),
            parent: process.parent().map(sysinfo::Pid::as_u32),
            name: process.name().to_string_lossy().into_owned(),
            executable: process
                .exe()
                .map(|path| path.to_string_lossy().into_owned()),
            memory: process.memory(),
        })
        .collect()
}

struct Identity {
    key: String,
    name: String,
}

fn basename(value: &str) -> &str {
    value.rsplit(['/', '\\']).next().unwrap_or(value)
}

fn without_exe(value: &str) -> &str {
    if value
        .get(value.len().saturating_sub(EXE.len())..)
        .is_some_and(|suffix| suffix.eq_ignore_ascii_case(EXE))
    {
        &value[..value.len() - EXE.len()]
    } else {
        value
    }
}

fn bundle_identity(path: &str) -> Option<Identity> {
    // The outer app owns nested login items and Helper.app bundles as well.
    let normalized = path.replace('\\', "/");
    for (index, _) in normalized.match_indices(BUNDLE) {
        let end = index + BUNDLE.len();
        if end != normalized.len() && normalized.as_bytes()[end] != b'/' {
            continue;
        }
        let bundle = &normalized[..end];
        let name = basename(bundle).strip_suffix(BUNDLE)?;
        return Some(Identity {
            key: format!("bundle:{bundle}"),
            name: name.to_owned(),
        });
    }
    None
}

fn executable_identity(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    // Windows paths are case insensitive; Unix paths keep their case.
    if normalized.as_bytes().get(1) == Some(&b':') || normalized.starts_with("//") {
        normalized.to_lowercase()
    } else {
        normalized
    }
}

/// This process's key for app IDs: random, so an ID is stable while the app
/// runs and cannot be matched to a path or to another session's IDs.
fn session_key() -> &'static RandomState {
    static KEY: OnceLock<RandomState> = OnceLock::new();
    KEY.get_or_init(RandomState::new)
}

fn opaque_id(key: &str) -> String {
    keyed_id(session_key(), key)
}

/// The ID of `key` under `session`: a keyed SipHash, so it hides the path.
fn keyed_id(session: &RandomState, key: &str) -> String {
    format!("app-{:016x}", session.hash_one(key))
}

fn readable_name(raw: &str) -> String {
    let raw = without_exe(basename(raw));
    match raw.to_ascii_lowercase().as_str() {
        "chrome" | "google-chrome" | "google-chrome-stable" => "Google Chrome",
        "chromium" | "chromium-browser" => "Chromium",
        "firefox" | "firefox-bin" => "Firefox",
        "code" | "code-insiders" => "Visual Studio Code",
        "msedge" | "microsoft-edge" | "microsoft-edge-stable" => "Microsoft Edge",
        "brave" | "brave-browser" => "Brave Browser",
        "safari" => "Safari",
        "cursor" => "Cursor",
        "slack" => "Slack",
        "discord" => "Discord",
        "teams" | "ms-teams" => "Microsoft Teams",
        "zoom" => "Zoom",
        "spotify" => "Spotify",
        _ => raw,
    }
    .to_owned()
}

fn direct_identity(process: &ProcessUsage) -> Identity {
    if let Some(bundle) = process.executable.as_deref().and_then(bundle_identity) {
        return bundle;
    }
    let executable = process
        .executable
        .as_deref()
        .map(basename)
        .filter(|name| !name.is_empty())
        .unwrap_or(&process.name);
    let name = readable_name(executable);
    let key = process
        .executable
        .as_deref()
        .filter(|path| !path.is_empty())
        .map_or_else(
            || format!("name:{}", name.to_lowercase()),
            |path| format!("exe:{}", executable_identity(path)),
        );
    Identity { key, name }
}

fn is_helper(process: &ProcessUsage) -> bool {
    let name = format!(
        "{} {}",
        process.name,
        process.executable.as_deref().map(basename).unwrap_or("")
    )
    .to_ascii_lowercase();
    [
        "helper",
        "renderer",
        "webcontent",
        "web content",
        "gpu-process",
        "crashpad",
        "plugin-container",
        "webkitwebprocess",
        "webkitnetworkprocess",
        "webkit.networking",
        "qtwebengineprocess",
    ]
    .iter()
    .any(|kind| name.contains(kind))
}

fn owns_itself(process: &ProcessUsage) -> bool {
    process
        .executable
        .as_deref()
        .and_then(bundle_identity)
        .is_some()
        || !is_helper(process)
}

fn identity(process: &ProcessUsage, processes: &HashMap<u32, ProcessUsage>) -> Identity {
    if owns_itself(process) {
        return direct_identity(process);
    }
    let mut current = process;
    let mut visited = HashSet::new();
    for _ in 0..MAX_HELPER_DEPTH {
        if !visited.insert(current.pid) {
            break;
        }
        let Some(parent) = current.parent.and_then(|pid| processes.get(&pid)) else {
            break;
        };
        // Do not assign an orphan helper to launchd, systemd or the kernel.
        if parent.pid <= 1 {
            break;
        }
        if owns_itself(parent) {
            return direct_identity(parent);
        }
        current = parent;
    }
    direct_identity(process)
}

/// Groups processes into apps, largest first, at most `limit`.
pub(crate) fn aggregate(
    observations: impl IntoIterator<Item = ProcessUsage>,
    secrets: &Secrets,
    limit: usize,
) -> Vec<AppUsage> {
    let processes: HashMap<_, _> = observations.into_iter().map(|p| (p.pid, p)).collect();
    let mut apps = BTreeMap::<String, AppUsage>::new();
    for process in processes.values() {
        let id = identity(process, &processes);
        let public_id = opaque_id(&id.key);
        let app = apps.entry(id.key).or_insert_with(|| AppUsage {
            id: public_id,
            name: secrets
                .scrub(&id.name)
                .unwrap_or_else(|| "Unknown application".into()),
            ..Default::default()
        });
        app.memory = app.memory.saturating_add(process.memory);
        app.processes += 1;
    }
    let mut apps: Vec<_> = apps.into_values().collect();
    apps.sort_by(|a, b| {
        b.memory
            .cmp(&a.memory)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id))
    });
    apps.truncate(limit);
    apps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: u32, parent: Option<u32>, name: &str, path: &str, memory: u64) -> ProcessUsage {
        ProcessUsage {
            pid,
            parent,
            name: name.into(),
            executable: Some(path.into()),
            memory,
        }
    }

    fn group(processes: impl IntoIterator<Item = ProcessUsage>) -> Vec<AppUsage> {
        aggregate(processes, &Secrets::default(), usize::MAX)
    }

    #[test]
    fn nested_mac_helpers_belong_to_outer_application() {
        let apps = group([
            process(
                10,
                Some(1),
                "Google Chrome",
                "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
                100,
            ),
            process(
                11,
                Some(10),
                "Google Chrome Helper",
                "/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Helper.app/Contents/MacOS/Google Chrome Helper",
                200,
            ),
        ]);
        assert_eq!(
            apps,
            [AppUsage {
                id: opaque_id("bundle:/Applications/Google Chrome.app"),
                name: "Google Chrome".into(),
                memory: 300,
                processes: 2,
            }]
        );
    }

    #[test]
    fn same_named_installs_keep_distinct_stable_opaque_ids() {
        let apps = group([
            process(10, None, "editor", "/opt/stable/editor", 10),
            process(11, None, "editor", "/opt/nightly/editor", 20),
        ]);
        assert_eq!(apps.len(), 2);
        assert_ne!(apps[0].id, apps[1].id);
        let restarted = group([process(99, None, "editor", "/opt/stable/editor", 999)]);
        assert_eq!(apps[1].id, restarted[0].id);
    }

    #[test]
    fn ids_are_stable_in_a_session_and_differ_between_sessions() {
        let path = "/Users/someone/Applications/Private Project.app";
        assert_eq!(opaque_id(path), opaque_id(path));
        let (one, two) = (RandomState::new(), RandomState::new());
        assert_ne!(keyed_id(&one, path), keyed_id(&two, path));
        let id = opaque_id(path);
        assert!(!id.contains("someone") && !id.contains("Private"));
        assert_eq!(id.len(), "app-".len() + 16);
    }

    #[test]
    fn windows_paths_differing_only_in_case_group_together() {
        let apps = group([
            process(10, None, "editor.exe", "C:\\Tools\\Editor.exe", 10),
            process(11, None, "editor.exe", "c:\\tools\\editor.EXE", 20),
        ]);
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].processes, 2);
    }

    #[test]
    fn helpers_inherit_parents_without_losing_accounting() {
        let apps = group([
            process(10, Some(1), "firefox", "/usr/bin/firefox", 100),
            process(
                11,
                Some(10),
                "Web Content",
                "/usr/lib/firefox/plugin-container",
                200,
            ),
            process(12, Some(11), "helper", "/usr/lib/firefox/helper", 50),
            process(20, Some(1), "bash", "/usr/bin/bash", 10),
        ]);
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].name, "Firefox");
        assert_eq!(apps[0].processes, 3);
        assert_eq!(apps.iter().map(|a| a.memory).sum::<u64>(), 360);
    }

    #[test]
    fn cycles_and_missing_parents_do_not_drop_processes() {
        let apps = group([
            process(10, Some(11), "helper", "/bin/helper", 10),
            process(11, Some(10), "helper", "/bin/helper", 20),
            process(12, Some(99), "renderer", "/bin/renderer", 30),
        ]);
        assert_eq!(apps.iter().map(|a| a.processes).sum::<usize>(), 3);
        assert_eq!(apps.iter().map(|a| a.memory).sum::<u64>(), 60);
    }

    #[test]
    fn limit_keeps_the_largest_apps() {
        let apps = aggregate(
            (0..20).map(|i| process(i + 2, None, "app", &format!("/opt/app{i}"), u64::from(i))),
            &Secrets::default(),
            APP_LIMIT,
        );
        assert_eq!(apps.len(), APP_LIMIT);
        assert_eq!(apps[0].memory, 19);
        assert_eq!(apps[9].memory, 10);
    }
}
