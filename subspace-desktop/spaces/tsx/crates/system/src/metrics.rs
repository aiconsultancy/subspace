//! CPU, memory and network readings, and the collector that takes them.

use std::time::{Duration, Instant};

use serde::Serialize;
use sysinfo::{
    MemoryRefreshKind, Networks, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind,
};

use crate::apps::{self, APP_LIMIT, Apps};
use crate::privacy::Secrets;

/// The `subspace.system.metrics` reading. `null` means unavailable.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Metrics {
    /// Milliseconds since the Unix epoch.
    pub at: u64,
    /// Overall CPU use in percent.
    pub cpu: Option<f32>,
    /// CPU use per logical core in percent; empty when `cpu` is `null`.
    pub cores: Vec<f32>,
    pub memory: Memory,
    pub network: Network,
}

/// Memory in bytes.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    pub used: Option<u64>,
    pub total: Option<u64>,
    pub swap_used: Option<u64>,
    pub swap_total: Option<u64>,
}

/// Bytes per second over non-loopback interfaces; `null` until a baseline.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Network {
    pub rx: Option<f64>,
    pub tx: Option<f64>,
}

/// Owns the `sysinfo` state the readings need between samples.
pub(crate) struct Collector {
    system: System,
    networks: Networks,
    cpu_since: Option<Instant>,
    network_since: Option<Instant>,
    secrets: Secrets,
}

impl Collector {
    pub(crate) fn new() -> Option<Self> {
        Some(Self {
            system: System::new().ok()?,
            networks: Networks::new().ok()?,
            cpu_since: None,
            network_since: None,
            secrets: Secrets::current(),
        })
    }

    /// Takes a metrics reading. The first call waits once for a CPU baseline;
    /// network rates stay `null` until the second call.
    pub(crate) fn metrics(&mut self) -> Metrics {
        if self.cpu_since.is_none() {
            self.system.refresh_cpu_usage();
            std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
            self.cpu_since = Some(Instant::now());
        }
        self.system.refresh_cpu_usage();
        self.system
            .refresh_memory_specifics(MemoryRefreshKind::everything());
        self.networks.refresh(true);
        let now = Instant::now();
        let elapsed = self.network_since.map(|since| now - since);
        self.network_since = Some(now);

        let (mut rx, mut tx) = (0_u64, 0_u64);
        for (name, data) in &self.networks {
            if !is_loopback(&name.to_string_lossy()) {
                rx = rx.saturating_add(data.received());
                tx = tx.saturating_add(data.transmitted());
            }
        }
        let cores: Vec<f32> = self.system.cpus().iter().map(sysinfo::Cpu::usage).collect();
        Metrics {
            at: crate::now_ms(),
            cpu: (!cores.is_empty()).then(|| self.system.global_cpu_usage()),
            cores,
            memory: Memory {
                used: known(self.system.used_memory(), self.system.total_memory()),
                total: nonzero(self.system.total_memory()),
                swap_used: known(self.system.used_swap(), self.system.total_swap()),
                swap_total: nonzero(self.system.total_swap()),
            },
            network: Network {
                rx: elapsed.and_then(|elapsed| rate(rx, elapsed)),
                tx: elapsed.and_then(|elapsed| rate(tx, elapsed)),
            },
        }
    }

    /// Takes an apps reading with a memory-only process refresh.
    pub(crate) fn apps(&mut self) -> Apps {
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_memory()
                .with_exe(UpdateKind::OnlyIfNotSet),
        );
        Apps {
            at: crate::now_ms(),
            apps: apps::aggregate(apps::observe(&self.system), &self.secrets, APP_LIMIT),
        }
    }
}

/// Loopback interfaces: `lo` on Linux, `lo0` and similar on macOS and BSD,
/// and Windows' "Loopback Pseudo-Interface".
fn is_loopback(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("loopback")
        || lower
            .strip_prefix("lo")
            .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit()))
}

/// Bytes per second, or `None` without a usable interval.
fn rate(bytes: u64, elapsed: Duration) -> Option<f64> {
    let seconds = elapsed.as_secs_f64();
    (seconds > 0.0).then(|| bytes as f64 / seconds)
}

fn nonzero(value: u64) -> Option<u64> {
    (value > 0).then_some(value)
}

/// A used amount is only known when the platform reports a total.
fn known(used: u64, total: u64) -> Option<u64> {
    (total > 0).then_some(used)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_interfaces_are_excluded() {
        for name in ["lo", "lo0", "Loopback Pseudo-Interface 1"] {
            assert!(is_loopback(name), "{name}");
        }
        for name in ["en0", "eth0", "wlan0", "lowpan0"] {
            assert!(!is_loopback(name), "{name}");
        }
    }

    #[test]
    fn rates_need_an_interval() {
        assert_eq!(rate(2048, Duration::from_secs(2)), Some(1024.0));
        assert_eq!(rate(10, Duration::ZERO), None);
    }

    #[test]
    fn unknown_totals_are_null_not_zero() {
        assert_eq!(nonzero(0), None);
        assert_eq!(known(5, 0), None);
        assert_eq!(known(0, 8), Some(0));
        let json = crate::to_json(&Metrics::default());
        assert_eq!(json["cpu"], serde_json::Value::Null);
        assert_eq!(json["memory"]["swapUsed"], serde_json::Value::Null);
        assert_eq!(json["network"]["rx"], serde_json::Value::Null);
    }

    #[test]
    fn network_rates_start_null_then_have_a_baseline() {
        let mut collector = Collector::new().expect("sysinfo supports this platform");
        let first = collector.metrics();
        assert_eq!(first.network, Network::default());
        assert!(first.cpu.is_some());
        let second = collector.metrics();
        assert!(second.network.rx.is_some() && second.network.tx.is_some());
        assert_eq!(second.cores.len(), first.cores.len());
    }
}
