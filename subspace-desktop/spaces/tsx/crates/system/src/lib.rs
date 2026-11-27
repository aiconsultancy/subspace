//! Read-only system readings for TSX Spaces: metrics, apps and facts.
//!
//! The crate has no GPUI and no nsh dependency. The host exposes each tier
//! under its host-channel name ([`Tier::host_name`]) to packages granted it.
//! See `docs/changes/tsx-spaces/spec.md` §5 for the contract.
//!
//! Privacy: no reading holds a PID, command line, environment variable,
//! executable or working folder path, user ID, host name, user name, serial
//! number, MAC or IP address. App IDs are opaque hashes.

mod apps;
mod facts;
mod metrics;
mod privacy;
mod sampler;

use serde::Serialize;

pub use apps::{APP_LIMIT, AppUsage, Apps};
pub use facts::{Display, Facts, FactsInput, read_facts};
pub use metrics::{Memory, Metrics, Network};
pub use sampler::{Listener, Sampler, Subscription};

/// A group of readings sampled together.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tier {
    /// CPU, memory and network, every second.
    Metrics,
    /// Top apps by resident memory, every two seconds.
    Apps,
    /// Facts about the machine, read on demand.
    Facts,
}

impl Tier {
    /// Every tier, in host-name order.
    pub const ALL: [Tier; 3] = [Tier::Metrics, Tier::Apps, Tier::Facts];

    /// The host-channel name a package calls and declares.
    pub const fn host_name(self) -> &'static str {
        match self {
            Tier::Metrics => "subspace.system.metrics",
            Tier::Apps => "subspace.system.apps",
            Tier::Facts => "subspace.system.facts",
        }
    }

    /// The tier a host-channel name names.
    // qual:api — the TSX Space host routes calls with it; tests cover it.
    pub fn from_host_name(name: &str) -> Option<Tier> {
        Tier::ALL.into_iter().find(|tier| tier.host_name() == name)
    }
}

/// Milliseconds since the Unix epoch.
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// Serialises a reading as the JSON value sent to a package.
pub fn to_json<T: Serialize>(reading: &T) -> serde_json::Value {
    serde_json::to_value(reading).unwrap_or(serde_json::Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_names_round_trip() {
        for tier in Tier::ALL {
            assert_eq!(Tier::from_host_name(tier.host_name()), Some(tier));
        }
        assert_eq!(Tier::from_host_name("subspace.system.other"), None);
    }

    /// Every string in a JSON value.
    fn strings(value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::String(text) => out.push(text.clone()),
            serde_json::Value::Array(items) => items.iter().for_each(|item| strings(item, out)),
            serde_json::Value::Object(map) => map.iter().for_each(|(key, item)| {
                out.push(key.clone());
                strings(item, out);
            }),
            _ => {}
        }
    }

    #[test]
    fn app_readings_hold_no_pid_path_command_line_or_personal_name() {
        let secrets = privacy::Secrets::new(["alice".into(), "alices-laptop".into()]);
        let process = |pid, name: &str, path: &str| apps::ProcessUsage {
            pid,
            parent: None,
            name: name.into(),
            executable: Some(path.into()),
            memory: 1,
        };
        let apps = apps::aggregate(
            [
                process(
                    48_213,
                    "secret-tool --token=abc",
                    "/Users/alice/bin/secret-tool",
                ),
                process(
                    48_214,
                    "Alice's Notes",
                    "/Users/alice/Applications/Alice's Notes.app/Contents/MacOS/n",
                ),
                process(48_215, "runner", "/home/alices-laptop/runner"),
                process(48_216, "editor", "C:\\Users\\alice\\editor.exe"),
            ],
            &secrets,
            APP_LIMIT,
        );
        let json = to_json(&Apps { at: 1, apps });
        let text = json.to_string();
        for forbidden in [
            "48213", "48214", "/", "\\\\", "alice", "Alice", "--token", "Users", "home",
        ] {
            assert!(!text.contains(forbidden), "{forbidden} leaked in {text}");
        }
        let mut keys: Vec<_> = json["apps"][0]
            .as_object()
            .expect("app")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        assert_eq!(keys, ["id", "memory", "name", "processes"]);
    }

    #[test]
    fn live_readings_hold_no_host_name_user_name_or_path() {
        let secrets = privacy::Secrets::current();
        let sampler = Sampler::new(std::sync::Arc::new(|_, _: &serde_json::Value| {}));
        for tier in Tier::ALL {
            let mut found = Vec::new();
            strings(&sampler.latest(tier), &mut found);
            for text in found {
                assert_eq!(
                    secrets.scrub(&text).as_deref(),
                    Some(text.trim()),
                    "{tier:?}: {text}"
                );
            }
        }
    }
}
