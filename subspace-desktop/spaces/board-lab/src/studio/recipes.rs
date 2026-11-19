//! Built-in and custom recipe lookup, drafts, and contained execution.

mod catalog;

use super::controls::{
    add_control, parse_id, snapshot_matches, validate_controls, validate_parameters,
    validate_title, validate_view,
};
use super::dataset::{bound_bytes, parse_dataset, validate_dataset};
use super::limits::{MAX_INPUT_BYTES, MAX_RESULT_BYTES, SNAPSHOT_RECIPE};
use super::store::{draft_id, now_rfc3339};
use crate::contracts::{EnvironmentInfo, Parameters, Preset, RunResult};
use crate::environments::EnvironmentManager;
use anyhow::{Context, Result, bail};
use catalog::{Recipe, resolve, resolve_profile};
use serde_json::Value;
use std::path::Path;
use std::time::Instant;

pub(super) fn new_draft(root: &Path, profile: &str) -> Result<Preset> {
    let recipe = resolve_profile(root, profile)?;
    Ok(Preset {
        id: draft_id(&recipe.id),
        title: recipe.title.to_owned(),
        environment: recipe.environment.to_owned(),
        recipe: if profile.starts_with("custom:") {
            profile.into()
        } else {
            recipe.id.to_owned()
        },
        parameters: Parameters::default(),
        controls: Vec::new(),
        view: "bar".into(),
        snapshot: None,
        revision: 0,
        updated_at: now_rfc3339(),
    })
}

pub(super) fn run(root: &Path, preset: &Preset) -> Result<RunResult> {
    let recipe = prepared(root, preset)?;
    let started = Instant::now();
    let (stdout, info) = execute(root, preset, &recipe)?;
    finish_run(preset, stdout, info, started)
}

pub(super) fn validate_save(root: &Path, preset: &Preset) -> Result<()> {
    parse_id(&preset.id)?;
    validate_title(&preset.title)?;
    validate_view(&preset.view)?;
    if preset.recipe == SNAPSHOT_RECIPE {
        validate_snapshot(preset)
    } else {
        validate_live(root, preset)
    }
}

pub(super) fn add_bound_control(root: &Path, preset: &mut Preset, parameter: &str) -> Result<()> {
    let services = resolve(root, &preset.recipe)?.services;
    add_control(preset, parameter, &services)
}

pub(super) fn source(root: &Path, reference: &str) -> Result<String> {
    Ok(resolve(root, reference)?.source)
}

pub(super) fn network(root: &Path, reference: &str) -> Result<bool> {
    Ok(resolve(root, reference)?.network)
}

fn prepared(root: &Path, preset: &Preset) -> Result<Recipe> {
    parse_id(&preset.id)?;
    let recipe = resolve(root, &preset.recipe)?;
    ensure_environment(preset, &recipe)?;
    validate_parameters(&preset.parameters, &recipe.services)?;
    validate_controls(preset, &recipe.services)?;
    validate_view(&preset.view)?;
    let input = serde_json::to_value(&preset.parameters).context("serializing run input")?;
    bound_bytes(&input, MAX_INPUT_BYTES, "run input")?;
    Ok(recipe)
}

fn execute(root: &Path, preset: &Preset, recipe: &Recipe) -> Result<(String, EnvironmentInfo)> {
    let manager = EnvironmentManager::new(root.join("environments"));
    let info = configured(&manager, &preset.environment)?;
    let stdout = worker_stdout(root, &manager, preset, recipe)?;
    if stdout.len() > MAX_RESULT_BYTES {
        bail!("worker result exceeds the {MAX_RESULT_BYTES} byte limit");
    }
    Ok((stdout, info))
}

fn finish_run(
    preset: &Preset,
    stdout: String,
    info: EnvironmentInfo,
    started: Instant,
) -> Result<RunResult> {
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let mut dataset = parse_dataset(&stdout)?;
    dataset
        .provenance
        .push_str(&provenance_suffix(preset, &info));
    validate_dataset(&dataset)?;
    Ok(RunResult {
        dataset,
        stdout,
        runtime: format!(
            "{} · {:?} {}",
            preset.environment, info.spec.runtime, info.spec.version
        ),
        elapsed_ms,
    })
}

fn provenance_suffix(preset: &Preset, info: &EnvironmentInfo) -> String {
    format!(
        "; recipe_ref={}; runtime={:?} {}; env_revision={}; run_at={}",
        preset.recipe,
        info.spec.runtime,
        info.spec.version,
        info.project
            .file_name()
            .unwrap_or_default()
            .to_string_lossy(),
        now_rfc3339()
    )
}

fn worker_stdout(
    root: &Path,
    manager: &EnvironmentManager,
    preset: &Preset,
    recipe: &Recipe,
) -> Result<String> {
    let input = input(preset)?;
    if recipe.network {
        network_stdout(root, manager, preset, recipe, &input)
    } else {
        manager.run_source(&preset.environment, &recipe.source, &input)
    }
}

fn network_stdout(
    root: &Path,
    manager: &EnvironmentManager,
    preset: &Preset,
    recipe: &Recipe,
    input: &Value,
) -> Result<String> {
    crate::net_probe::execute_at(root, manager, &preset.environment, &recipe.source, input)?
        ["stdout"]
        .as_str()
        .context("Network worker did not return stdout")
        .map(str::to_owned)
}

fn input(preset: &Preset) -> Result<Value> {
    serde_json::to_value(&preset.parameters).context("serializing run input")
}

fn configured(manager: &EnvironmentManager, environment: &str) -> Result<EnvironmentInfo> {
    manager
        .catalog()?
        .into_iter()
        .find(|info| info.spec.id == environment)
        .context("Recipe environment is not configured")
}

fn validate_snapshot(preset: &Preset) -> Result<()> {
    validate_dataset(
        preset
            .snapshot
            .as_ref()
            .context("A static canvas needs a snapshot")?,
    )?;
    if !preset.controls.is_empty() {
        bail!("Static canvas snapshots cannot have data-loading controls without a recipe");
    }
    Ok(())
}

fn validate_live(root: &Path, preset: &Preset) -> Result<()> {
    let recipe = resolve(root, &preset.recipe)?;
    ensure_environment(preset, &recipe)?;
    validate_parameters(&preset.parameters, &recipe.services)?;
    validate_controls(preset, &recipe.services)?;
    optional_snapshot(preset)
}

fn optional_snapshot(preset: &Preset) -> Result<()> {
    let Some(dataset) = &preset.snapshot else {
        return Ok(());
    };
    validate_dataset(dataset)?;
    if snapshot_matches(preset, dataset) {
        Ok(())
    } else {
        bail!(
            "Snapshot parameters differ from the saved recipe. Run again, or save the definition without a snapshot."
        )
    }
}

fn ensure_environment(preset: &Preset, recipe: &Recipe) -> Result<()> {
    if preset.environment == recipe.environment {
        Ok(())
    } else {
        bail!(
            "recipe {} requires environment {}, not {}",
            recipe.id,
            recipe.environment,
            preset.environment
        )
    }
}
