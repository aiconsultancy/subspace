//! Disposable board-local environments. Provisioning is trusted host work;
//! every user source invocation goes through the parent MXC adapter.

use crate::contracts::{EnvironmentInfo, EnvironmentSpec, RuntimeKind};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

mod catalog;
mod execution;
mod extensions;
mod packages;
pub(crate) mod persistent;
mod provisioning;

const UV: &str = "0.13.5";
const PYTHON: &str = "3.12.8";
const PYTHON_ALT: &str = "3.11.11";
const NODE: &str = "22.14.0";
const PNPM: &str = "11.13.0";
const BUN: &str = "1.4.2";
const JQ: &str = "1.8.2";
const REQUIRED_MISE_VERSION: &str = "2026.10.17";
pub(super) const MAX_CATALOG_BYTES: usize = 64 * 1024;
pub(super) const MAX_ENVIRONMENTS: usize = 16;
pub(super) const MAX_JOB_ATTEMPTS: u128 = 16;

#[derive(Clone)]
pub struct EnvironmentManager {
    pub root: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct ProjectState {
    version: String,
    executable: Option<PathBuf>,
}

impl EnvironmentManager {
    pub fn add_tool(&self, id: &str, tool: &str, version: &str) -> Result<EnvironmentInfo> {
        extensions::add_tool(&self.root, id, tool, version)
    }

    pub fn run_source(&self, id: &str, source: &str, input: &Value) -> Result<String> {
        self.run_source_with_proxy(id, source, input, None)
    }

    pub fn run_source_with_proxy(
        &self,
        id: &str,
        source: &str,
        input: &Value,
        proxy: Option<&str>,
    ) -> Result<String> {
        execution::run_source(&self.root, id, source, input, proxy)
    }
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn catalog(&self) -> Result<Vec<EnvironmentInfo>> {
        catalog::specs(&self.root)?
            .into_iter()
            .map(|mut spec| {
                if catalog::project(&self.root, &spec.id)
                    .join("environment.json")
                    .exists()
                {
                    spec.version = read_state(&catalog::project(&self.root, &spec.id))?.version;
                }
                catalog::info(&self.root, spec)
            })
            .collect()
    }

    pub fn register(&self, spec: EnvironmentSpec) -> Result<EnvironmentInfo> {
        validate_spec(&spec)?;
        let mut specs = catalog::specs(&self.root)?;
        if specs.iter().any(|existing| existing.id == spec.id) {
            bail!(
                "Environment already exists; use its version/package/tool operations to update it"
            );
        }
        if specs.len() >= MAX_ENVIRONMENTS {
            bail!("At most sixteen configured environments are supported");
        }
        specs.push(spec.clone());
        catalog::write_specs(&self.root, &specs)?;
        catalog::info(&self.root, spec)
    }

    pub fn provision(&self, id: &str) -> Result<EnvironmentInfo> {
        provisioning::provision(&self.root, id)
    }

    pub fn set_version(&self, id: &str, version: &str) -> Result<EnvironmentInfo> {
        validate_version(version)?;
        provisioning::set_version(&self.root, id, version)
    }

    pub fn add_package(&self, id: &str, package: &str) -> Result<EnvironmentInfo> {
        packages::add_package(&self.root, id, package)
    }
}

mod command;
mod filesystem;
mod installers;
mod validation;

use command::{bundled_mise, run_mise, verify_mise, which};
use filesystem::{
    add_node_package, add_python_package, copy_tree, link_runtime, read_state, valid_project,
    write_project, write_state,
};
use validation::{
    runtime_layout, runtime_tool_name, validate_package, validate_spec, validate_version,
};

pub(super) fn timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

#[cfg(test)]
mod tests {
    use super::command::has_required_mise_version;

    #[test]
    fn mise_version_check_requires_the_pinned_version() {
        assert!(has_required_mise_version("mise 2026.10.17 macos-arm64"));
        assert!(has_required_mise_version("v2026.10.17"));
        assert!(!has_required_mise_version("mise 2026.10.170"));
        assert!(!has_required_mise_version("mise 2026.10.16"));
    }
}
