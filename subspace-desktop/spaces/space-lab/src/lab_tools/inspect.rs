use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::environments::EnvironmentManager;
use crate::studio::Studio;

use super::helpers::preset;
use super::schema::Input;

pub(super) fn run(
    studio: &Studio,
    manager: &EnvironmentManager,
    root: &Path,
    input: Input,
) -> Result<Value> {
    match input.action.as_str() {
        "environments" => Ok(json!({"environments":manager.catalog()?})),
        "presets" => Ok(json!({"presets":studio.presets()?})),
        "network_policies" => Ok(json!({"network_policies":crate::net_probe::policies_at(root)?})),
        "dashboard" => Ok(json!({"layout":studio.dashboard()?})),
        "recipes" => recipes(root),
        "source" => source(studio, &input),
        "draft" => draft(studio, &input),
        "canvas" => canvas(studio, &input),
        _ => bail!("Unknown inspect action"),
    }
}

fn recipes(root: &Path) -> Result<Value> {
    Ok(
        json!({"recipes":crate::custom_recipes::list(root)?.into_iter().map(|recipe|json!({"id":recipe.id,"title":recipe.title,"reference":recipe.reference(),"environment":recipe.environment,"services":recipe.services,"network":recipe.network,"hash":recipe.hash})).collect::<Vec<_>>()}),
    )
}

fn source(studio: &Studio, input: &Input) -> Result<Value> {
    let preset = preset(studio, input)?;
    Ok(json!({"recipe":preset.recipe,"source":studio.recipe_source(&preset)?}))
}

fn draft(studio: &Studio, input: &Input) -> Result<Value> {
    Ok(json!({"preset":studio.new_draft(input.profile.as_deref().unwrap_or("market"))?}))
}

fn canvas(studio: &Studio, input: &Input) -> Result<Value> {
    let preset = preset(studio, input)?;
    let data = input
        .dataset
        .as_ref()
        .or(preset.snapshot.as_ref())
        .context("Run a recipe and supply dataset, or use a saved snapshot")?;
    Ok(
        json!({"canvas":crate::studio::export_canvas(&preset, data)?, "note":"Pass this supported source to Copilot open_canvas to preview. Saving to a board is a separate action."}),
    )
}
