//! Optional, explicitly pinned tools for a board environment.
//!
//! This module intentionally accepts only mise's built-in registries and
//! vendor-qualified identifiers. It does not install plugins or alter the
//! selected primary runtime.

use super::{EnvironmentInfo, copy_tree, run_mise};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::fs;
use std::path::Path;

const BUILTIN_TOOLS: &[&str] = &[
    "actionlint",
    "bat",
    "delta",
    "fd",
    "hyperfine",
    "jq",
    "rg",
    "ripgrep",
    "shellcheck",
    "watchexec",
    "yq",
];
const MAX_TOOL_NAME: usize = 160;
const PIN_PARTS: usize = 3;
pub(super) fn add_tool(
    root: &Path,
    id: &str,
    tool: &str,
    version: &str,
) -> Result<EnvironmentInfo> {
    validate_tool(tool)?;
    validate_pin(version)?;
    let spec = super::catalog::spec(root, id)?;
    let primary = super::runtime_tool_name(&spec.runtime);
    if [primary, "uv", "pnpm"].contains(&tool)
        || tool
            .rsplit_once(':')
            .is_some_and(|(_, name)| [primary, "uv", "pnpm"].contains(&name))
    {
        bail!("{tool} is a primary runtime or package manager; use set_version instead")
    }
    let current = super::catalog::project(root, id);
    if !current.join(".ready").is_file() {
        bail!("environment {id} is not ready")
    }
    let stage = super::provisioning::stage_path(root, id)?;
    if let Err(error) = (|| -> Result<()> {
        copy_tree(&current, &stage)?;
        let manifest = stage.join(".mise.toml");
        let mut config: toml::Value = toml::from_str(&fs::read_to_string(&manifest)?)
            .context("parse private mise configuration")?;
        let tools = config
            .get_mut("tools")
            .and_then(toml::Value::as_table_mut)
            .context("private mise configuration has no [tools] table")?;
        tools.insert(tool.to_owned(), toml::Value::String(version.to_owned()));
        fs::write(&manifest, toml::to_string_pretty(&config)?)?;
        super::provisioning::install_project(root, &stage, &spec)?;
        super::provisioning::activate_revision(&stage, &current)?;
        Ok(())
    })() {
        let _ = fs::remove_dir_all(&stage);
        return Err(error.context("extra tool update failed; active revision retained"));
    }
    super::catalog::info(root, spec)
}

pub(super) fn record_tool_paths(root: &Path, project: &Path) -> Result<()> {
    let output = run_mise(&root.join("bin/mise"), root, project, &["env", "--json"])?;
    let value: Value = serde_json::from_str(&output).context("parse mise environment JSON")?;
    let path = value
        .get("PATH")
        .and_then(Value::as_str)
        .context("mise environment did not provide PATH")?;
    let installs = fs::canonicalize(root.join("mise-data/installs"))?;
    let mut roots = Vec::new();
    for entry in std::env::split_paths(path) {
        let Ok(bin) = fs::canonicalize(&entry) else {
            continue;
        };
        let Ok(relative) = bin.strip_prefix(&installs) else {
            continue;
        };
        let mut parts = relative.components();
        let Some(tool) = parts.next() else { continue };
        let Some(version) = parts.next() else {
            continue;
        };
        let root = installs.join(tool.as_os_str()).join(version.as_os_str());
        if root.is_dir() && fs::canonicalize(&root).is_ok() && !roots.contains(&root) {
            roots.push(root);
        }
        // Archive releases can keep the executable in a nested directory.
        if bin.is_dir() && !roots.contains(&bin) {
            roots.push(bin);
        }
    }
    let bytes = serde_json::to_vec_pretty(&roots)?;
    let temporary = project.join("tool-roots.json.tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, project.join("tool-roots.json"))?;
    Ok(())
}

fn validate_tool(tool: &str) -> Result<()> {
    if tool.is_empty()
        || tool.len() > MAX_TOOL_NAME
        || tool.contains("://")
        || tool.chars().any(|c| {
            c.is_whitespace() || matches!(c, ';' | '|' | '&' | '$' | '`' | '\n' | '\r' | '"' | '\'')
        })
    {
        bail!("invalid extra tool identifier")
    }
    if let Some((backend, name)) = tool.split_once(':') {
        let allowed = match backend {
            "core" | "packslip" => BUILTIN_TOOLS.contains(&name),
            "aqua" | "github" => vendor_name(name),
            _ => false,
        };
        if !allowed {
            bail!("extra tool backend or vendor identifier is not allowed")
        }
        return Ok(());
    }
    if !BUILTIN_TOOLS.contains(&tool) {
        bail!("extra tool is not in the approved registry")
    }
    Ok(())
}

fn vendor_name(name: &str) -> bool {
    let mut pieces = name.split('/');
    let (Some(owner), Some(repo), None) = (pieces.next(), pieces.next(), pieces.next()) else {
        return false;
    };
    !owner.is_empty()
        && !owner.contains("..")
        && !repo.contains("..")
        && !repo.is_empty()
        && [owner, repo].iter().all(|part| {
            part.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
        })
}

fn validate_pin(version: &str) -> Result<()> {
    let pieces: Vec<_> = version.split('.').collect();
    if pieces.len() != PIN_PARTS
        || pieces
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()))
    {
        bail!("extra tool version must be an exact three-part pin")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_pin, validate_tool, vendor_name};

    #[test]
    fn extra_tool_validation_accepts_only_pinned_safe_sources() {
        validate_tool("rg").expect("approved builtin");
        validate_tool("github:owner/repo").expect("approved vendor");
        assert!(validate_tool("https://example.invalid/tool").is_err());
        assert!(validate_tool("owner/../repo").is_err());
        validate_pin("1.2.3").expect("exact pin");
        assert!(validate_pin("1.2").is_err());
        assert!(vendor_name("owner/repo"));
        assert!(!vendor_name("owner/../repo"));
    }
}
