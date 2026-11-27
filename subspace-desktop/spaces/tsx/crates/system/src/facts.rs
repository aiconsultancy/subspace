//! Facts about the machine, read on demand.

use serde::Serialize;

use crate::privacy::Secrets;

/// The `subspace.system.facts` reading. Each fact the platform cannot give
/// is `null`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Facts {
    /// Milliseconds since the Unix epoch.
    pub at: u64,
    pub os: Option<String>,
    pub os_version: Option<String>,
    pub kernel: Option<String>,
    pub architecture: Option<String>,
    pub machine_model: Option<String>,
    pub cpu_model: Option<String>,
    pub physical_cores: Option<usize>,
    pub logical_cores: Option<usize>,
    pub gpu_model: Option<String>,
    /// Bytes.
    pub memory_total: Option<u64>,
    /// Seconds since boot.
    pub uptime: Option<u64>,
    pub displays: Option<Vec<Display>>,
    /// The login shell's file name only, such as `zsh`.
    pub shell: Option<String>,
    pub subspace_version: Option<String>,
}

/// One display, as the host's windowing system reports it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Display {
    /// Logical pixels.
    pub width: u32,
    pub height: u32,
    pub scale: f32,
}

/// Facts only the host knows: this crate has no window system.
#[derive(Clone, Debug, Default)]
pub struct FactsInput {
    pub subspace_version: Option<String>,
    pub displays: Option<Vec<Display>>,
}

/// Unfiltered values, before the privacy rules apply.
#[derive(Clone, Debug, Default)]
struct RawFacts {
    os: Option<String>,
    os_version: Option<String>,
    kernel: Option<String>,
    architecture: Option<String>,
    machine_model: Option<String>,
    cpu_model: Option<String>,
    physical_cores: Option<usize>,
    logical_cores: Option<usize>,
    gpu_model: Option<String>,
    memory_total: Option<u64>,
    uptime: Option<u64>,
    shell_path: Option<String>,
}

/// Reads the facts. Takes a moment: it lists CPUs and GPUs.
pub fn read_facts(input: FactsInput) -> Facts {
    build(raw_facts(), input, &Secrets::current())
}

fn raw_facts() -> RawFacts {
    use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

    let system = System::new_with_specifics(
        RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::nothing())
            .with_memory(MemoryRefreshKind::nothing().with_ram()),
    )
    .ok();
    let cpus = system.as_ref().map_or(&[][..], System::cpus);
    let gpu_model = sysinfo::Gpus::new_with_refreshed_list()
        .ok()
        .and_then(|gpus| {
            (&gpus)
                .into_iter()
                .find_map(|gpu| gpu.model().map(str::to_owned))
        });
    let shell_variable = if cfg!(windows) { "COMSPEC" } else { "SHELL" };
    RawFacts {
        os: System::name().ok(),
        os_version: System::os_version().ok(),
        kernel: System::kernel_version().ok(),
        architecture: Some(System::cpu_arch()),
        machine_model: sysinfo::Product::name().ok(),
        cpu_model: cpus.first().map(|cpu| cpu.brand().to_owned()),
        physical_cores: System::physical_core_count().ok(),
        logical_cores: Some(cpus.len()),
        gpu_model,
        memory_total: system.as_ref().map(System::total_memory),
        uptime: System::uptime().ok(),
        shell_path: std::env::var(shell_variable).ok(),
    }
}

fn build(raw: RawFacts, input: FactsInput, secrets: &Secrets) -> Facts {
    let text = |value: Option<String>| value.and_then(|value| secrets.scrub(&value));
    let shell = raw.shell_path.as_deref().and_then(|path| {
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let name = name
            .strip_suffix(".exe")
            .or_else(|| name.strip_suffix(".EXE"))
            .unwrap_or(name);
        secrets.scrub(name)
    });
    Facts {
        at: crate::now_ms(),
        os: text(raw.os),
        os_version: text(raw.os_version),
        kernel: text(raw.kernel),
        architecture: text(raw.architecture),
        machine_model: text(raw.machine_model),
        cpu_model: text(raw.cpu_model),
        physical_cores: raw.physical_cores.filter(|&n| n > 0),
        logical_cores: raw.logical_cores.filter(|&n| n > 0),
        gpu_model: text(raw.gpu_model),
        memory_total: raw.memory_total.filter(|&n| n > 0),
        uptime: raw.uptime,
        displays: input.displays,
        shell,
        subspace_version: text(input.subspace_version),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_facts_are_null() {
        let facts = build(
            RawFacts::default(),
            FactsInput::default(),
            &Secrets::default(),
        );
        let json = crate::to_json(&facts);
        let object = json.as_object().expect("object");
        assert_eq!(object.len(), 15);
        for (key, value) in object {
            if key != "at" {
                assert!(value.is_null(), "{key} should be null");
            }
        }
    }

    #[test]
    fn shell_is_a_file_name() {
        let raw = |path: &str| RawFacts {
            shell_path: Some(path.into()),
            ..RawFacts::default()
        };
        let secrets = Secrets::default();
        let shell = |path| build(raw(path), FactsInput::default(), &secrets).shell;
        assert_eq!(shell("/opt/homebrew/bin/fish").as_deref(), Some("fish"));
        assert_eq!(
            shell("C:\\Windows\\system32\\cmd.exe").as_deref(),
            Some("cmd")
        );
    }

    #[test]
    fn reads_this_machine() {
        let facts = read_facts(FactsInput {
            subspace_version: Some("1.2.3".into()),
            displays: Some(vec![Display {
                width: 1512,
                height: 982,
                scale: 2.0,
            }]),
        });
        assert!(facts.logical_cores.is_some());
        assert!(facts.architecture.is_some());
        assert_eq!(facts.subspace_version.as_deref(), Some("1.2.3"));
    }
}
