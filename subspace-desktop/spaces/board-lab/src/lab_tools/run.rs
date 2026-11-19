use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::environments::EnvironmentManager;
use crate::studio::Studio;

use super::helpers::{preset, required};
use super::schema::Input;

const MAX_EXECUTE_SOURCE: usize = 32_768;

pub(super) fn run(
    studio: &Studio,
    manager: &EnvironmentManager,
    root: &Path,
    input: Input,
) -> Result<Value> {
    match input.action.as_str() {
        "recipe" => Ok(serde_json::to_value(studio.run(&preset(studio, &input)?)?)?),
        "network_probe" => crate::net_probe::local_probe_at(root),
        "execute" => execute(manager, root, input),
        _ => bail!("Unknown run action"),
    }
}

fn execute(manager: &EnvironmentManager, root: &Path, input: Input) -> Result<Value> {
    let source = input.source.context("source is required")?;
    if source.len() > MAX_EXECUTE_SOURCE {
        bail!("source exceeds 32 KiB");
    }
    let id = required(input.environment.as_deref(), "environment")?;
    let data = input.input.unwrap_or_else(|| json!({}));
    if input.network.unwrap_or(false) {
        return crate::net_probe::execute_at(root, manager, id, &source, &data);
    }
    let output = manager.run_source(id, &source, &data)?;
    Ok(json!({"stdout":output,"network":"denied","credentials":"none","sandbox":"MXC Seatbelt"}))
}

#[cfg(test)]
mod tests {
    use super::MAX_EXECUTE_SOURCE;
    use super::execute;
    use crate::environments::EnvironmentManager;
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn execute_rejects_source_over_32_kib_before_running() {
        let input = serde_json::from_value(json!({
            "action": "execute",
            "environment": "python-data",
            "source": "x".repeat(MAX_EXECUTE_SOURCE + 1),
        }))
        .expect("execute input");
        let manager = EnvironmentManager::new(Path::new("/unused").join("environments"));
        let error = execute(&manager, Path::new("/unused"), input).unwrap_err();
        assert!(error.to_string().contains("source exceeds 32 KiB"));
    }
}
