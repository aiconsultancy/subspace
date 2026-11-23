//! Prototype host-owned, exact-destination HTTP/CONNECT proxy.
//! Not a production gateway: no TLS interception, credential injection or
//! application-operation authorization. Local fixtures exercise confinement.

mod authority;
mod connect;
mod io;
mod probe;
mod proxy;
mod script;
mod serve;
mod store;
mod tunnel;

pub(crate) use authority::{Authority, public_ip};

#[cfg(test)]
mod tests;

use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

use crate::environments::EnvironmentManager;

use proxy::PolicyProxy;

// qual:api - prototype CLI reads default/file policies through the product root.
pub fn policies() -> Result<BTreeMap<String, Vec<String>>> {
    policies_at(&crate::workbench_root()?)
}

pub fn policies_at(root: &Path) -> Result<BTreeMap<String, Vec<String>>> {
    store::read(root)
}

// qual:api - prototype CLI writes destinations through the product root.
pub fn configure(
    manager: &EnvironmentManager,
    id: &str,
    destinations: Vec<String>,
) -> Result<Value> {
    configure_at(&crate::workbench_root()?, manager, id, destinations)
}

pub fn configure_at(
    root: &Path,
    manager: &EnvironmentManager,
    id: &str,
    destinations: Vec<String>,
) -> Result<Value> {
    store::write(root, manager, id, destinations)
}

// qual:api - prototype CLI runs proxy-only MXC execution through the product root.
pub fn execute(
    manager: &EnvironmentManager,
    id: &str,
    source: &str,
    input: &Value,
) -> Result<Value> {
    execute_at(&crate::workbench_root()?, manager, id, source, input)
}

pub fn execute_at(
    root: &Path,
    manager: &EnvironmentManager,
    id: &str,
    source: &str,
    input: &Value,
) -> Result<Value> {
    let policy = policies_at(root)?;
    let rules = policy
        .get(id)
        .context("Environment has no network policy")?
        .clone();
    let proxy = PolicyProxy::start(&rules)?;
    let (output, counts) = proxy
        .with_address(|address| manager.run_source_with_proxy(id, source, input, Some(address)))?;
    Ok(
        json!({"stdout":output,"destinations":rules,"proxy_requests":counts,"credentials":"none","network":"MXC proxy-only egress","note":"HTTPS CONNECT confines destination connections, not API paths or service operations."}),
    )
}

// qual:api - prototype CLI `--network-smoke` uses the product-root fixture.
pub fn local_probe() -> Result<Value> {
    local_probe_at(&crate::workbench_root()?)
}

pub fn local_probe_at(root: &Path) -> Result<Value> {
    probe::run(root)
}
