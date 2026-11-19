use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::environments::EnvironmentManager;
use crate::studio::Studio;

use super::helpers::required;
use super::schema::Input;

pub(super) fn run(
    _studio: &Studio,
    manager: &EnvironmentManager,
    root: &Path,
    input: Input,
) -> Result<Value> {
    if input.action == "register_environment" {
        return Ok(
            json!({"environment":manager.register(input.environment_spec.context("environment_spec is required")?)?}),
        );
    }
    let id = required(input.environment.as_deref(), "environment")?.to_owned();
    if input.action == "register_recipe" {
        return register_recipe(manager, root, &id, input);
    }
    if input.action == "configure_network" {
        return crate::net_probe::configure_at(
            root,
            manager,
            &id,
            input.destinations.context("destinations is required")?,
        );
    }
    mutate(manager, &id, input)
}

fn register_recipe(
    manager: &EnvironmentManager,
    root: &Path,
    id: &str,
    input: Input,
) -> Result<Value> {
    if !manager.catalog()?.iter().any(|info| info.spec.id == id) {
        bail!("Register the environment first");
    }
    let recipe = crate::custom_recipes::register(
        root,
        crate::custom_recipes::RecipeDefinition {
            id: required(input.id.as_deref(), "id")?.into(),
            title: required(input.title.as_deref(), "title")?.into(),
            environment: id.into(),
            source: required(input.source.as_deref(), "source")?.into(),
            services: input.services.unwrap_or_default(),
            network: input.network.unwrap_or(false),
        },
    )?;
    Ok(
        json!({"reference":recipe.reference(),"source_hash":recipe.hash,"preset":Studio::new(root.to_owned()).new_draft(&recipe.reference())?}),
    )
}

fn mutate(manager: &EnvironmentManager, id: &str, input: Input) -> Result<Value> {
    let info = match input.action.as_str() {
        "provision" => manager.provision(id)?,
        "set_version" => manager.set_version(id, required(input.version.as_deref(), "version")?)?,
        "add_package" => manager.add_package(id, required(input.package.as_deref(), "package")?)?,
        "add_tool" => manager.add_tool(
            id,
            required(input.tool.as_deref(), "tool")?,
            required(input.version.as_deref(), "version")?,
        )?,
        _ => bail!("Unknown environment action"),
    };
    Ok(json!({"environment":info}))
}
