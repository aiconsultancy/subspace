//! Built-in recipe catalog and custom recipe references.

use super::super::limits::{BUN_SERVICES, MARKET_SERVICES, NODE_SERVICES, SNAPSHOT_RECIPE};
use anyhow::{Result, bail};
use std::path::Path;

const MARKET_SOURCE: &str = include_str!("../../../recipes/market.py");
const SERVICES_SOURCE: &str = include_str!("../../../recipes/services.js");
const TYPESCRIPT_SOURCE: &str = include_str!("../../../recipes/typescript.ts");

pub(super) struct Recipe {
    pub id: String,
    pub environment: String,
    pub title: String,
    pub source: String,
    pub services: Vec<String>,
    pub network: bool,
}

pub(super) fn resolve_profile(root: &Path, profile: &str) -> Result<Recipe> {
    if profile.starts_with("custom:") {
        resolve(root, profile)
    } else {
        recipe_for_profile(profile)
    }
}

pub(super) fn resolve(root: &Path, reference: &str) -> Result<Recipe> {
    if reference.starts_with("custom:") {
        Ok(from_custom(crate::custom_recipes::load(root, reference)?))
    } else {
        recipe_by_id(reference)
    }
}

fn from_custom(recipe: crate::custom_recipes::CustomRecipe) -> Recipe {
    Recipe {
        id: recipe.id,
        title: recipe.title,
        environment: recipe.environment,
        source: recipe.source,
        services: recipe.services,
        network: recipe.network,
    }
}

fn recipes() -> [Recipe; 3] {
    let make = |id: &str, environment: &str, title: &str, source: &str, services: &[&str]| Recipe {
        id: id.into(),
        environment: environment.into(),
        title: title.into(),
        source: source.into(),
        services: services.iter().map(|value| (*value).to_owned()).collect(),
        network: false,
    };
    [
        make(
            "market",
            "python-data",
            "Market profile draft",
            MARKET_SOURCE,
            MARKET_SERVICES,
        ),
        make(
            "services",
            "node-services",
            "Service profile draft",
            SERVICES_SOURCE,
            NODE_SERVICES,
        ),
        make(
            "typescript",
            "bun-notebook",
            "TypeScript notebook draft",
            TYPESCRIPT_SOURCE,
            BUN_SERVICES,
        ),
    ]
}

fn recipe_for_profile(profile: &str) -> Result<Recipe> {
    recipes()
        .into_iter()
        .find(|recipe| recipe.id == profile)
        .ok_or_else(|| {
            anyhow::anyhow!("unknown profile {profile:?}; expected market, services, or typescript")
        })
}

fn recipe_by_id(recipe: &str) -> Result<Recipe> {
    if recipe == SNAPSHOT_RECIPE || recipe.is_empty() {
        bail!("preset is a static canvas snapshot and has no refreshable recipe");
    }
    recipes()
        .into_iter()
        .find(|item| item.id == recipe)
        .ok_or_else(|| anyhow::anyhow!("unknown recipe {recipe:?}"))
}
