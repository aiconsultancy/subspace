use anyhow::{Result, ensure};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::environments::EnvironmentManager;

use super::authority::{Authority, public_port};

const POLICY_FILE_MAX: usize = 8192;
const MAX_DESTINATIONS: usize = 8;

pub(super) fn read(root: &Path) -> Result<BTreeMap<String, Vec<String>>> {
    let file = root.join("network-policies.json");
    if !file.exists() {
        return Ok(defaults());
    }
    let bytes = fs::read(&file)?;
    ensure!(
        bytes.len() < POLICY_FILE_MAX,
        "Network policy file exceeds 8 KiB"
    );
    Ok(serde_json::from_slice(&bytes)?)
}

fn defaults() -> BTreeMap<String, Vec<String>> {
    BTreeMap::from([
        ("python-data".into(), vec!["api.github.com:443".into()]),
        ("node-services".into(), vec!["api.github.com:443".into()]),
        ("bun-notebook".into(), vec!["api.github.com:443".into()]),
        ("python-alt".into(), Vec::new()),
        ("cli-lab".into(), Vec::new()),
    ])
}

pub(super) fn write(
    root: &Path,
    manager: &EnvironmentManager,
    id: &str,
    destinations: Vec<String>,
) -> Result<Value> {
    ensure!(
        manager.catalog()?.iter().any(|info| info.spec.id == id),
        "Unknown environment"
    );
    validate_destinations(&destinations)?;
    let mut policy = read(root)?;
    policy.insert(id.into(), destinations);
    persist(root, &policy)?;
    Ok(serde_json::json!({"network_policies":policy}))
}

fn validate_destinations(destinations: &[String]) -> Result<()> {
    ensure!(
        destinations.len() <= MAX_DESTINATIONS,
        "Too many network destinations"
    );
    for value in destinations {
        let rule = Authority::parse(value)?;
        ensure!(
            public_port(rule.port),
            "Only ports 80 and 443 are supported"
        );
    }
    Ok(())
}

fn persist(root: &Path, policy: &BTreeMap<String, Vec<String>>) -> Result<()> {
    let path = root.join("network-policies.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&temp, serde_json::to_vec_pretty(policy)?)?;
    fs::rename(temp, path)?;
    Ok(())
}
