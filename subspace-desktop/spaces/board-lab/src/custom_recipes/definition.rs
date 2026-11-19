//! Recipe identity, bounds, and content-addressed hashing.

use super::RecipeDefinition;
use anyhow::{Context, Result, bail};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub(super) const MAX_SOURCE: usize = 32 * 1024;
pub(super) const MAX_RECORD: usize = 48 * 1024;
pub(super) const MAX_IDS: usize = 32;
pub(super) const MAX_REVS: usize = 64;
const MAX_TITLE_LEN: usize = 160;
const MAX_SERVICES: usize = 16;
const MAX_SERVICE_LEN: usize = 80;
const MAX_ID_LEN: usize = 48;
const HASH_LEN: usize = 64;

const SECRET_MARKERS: [&str; 5] = [
    "-----begin",
    "password=",
    "api_key",
    "secret=",
    "authorization:",
];

#[derive(Serialize)]
struct HashBody<'a> {
    id: &'a str,
    title: &'a str,
    environment: &'a str,
    source: &'a str,
    services: &'a [String],
    network: bool,
}

impl RecipeDefinition {
    pub(super) fn validate(&self) -> Result<()> {
        check_id(&self.id)?;
        check_title(&self.title)?;
        check_id(&self.environment)?;
        check_source(&self.source)?;
        check_services(&self.services)?;
        deny_secret_fields(self)
    }

    pub(super) fn fingerprint(&self) -> Result<String> {
        hash_body(HashBody {
            id: &self.id,
            title: &self.title,
            environment: &self.environment,
            source: &self.source,
            services: &self.services,
            network: self.network,
        })
    }

    pub(super) fn into_recipe(self, hash: String) -> super::CustomRecipe {
        super::CustomRecipe {
            id: self.id,
            title: self.title,
            environment: self.environment,
            source: self.source,
            services: self.services,
            network: self.network,
            hash,
        }
    }
}

pub(super) fn fingerprint_recipe(recipe: &super::CustomRecipe) -> Result<String> {
    hash_body(HashBody {
        id: &recipe.id,
        title: &recipe.title,
        environment: &recipe.environment,
        source: &recipe.source,
        services: &recipe.services,
        network: recipe.network,
    })
}

fn hash_body(body: HashBody<'_>) -> Result<String> {
    let bytes = serde_json::to_vec(&body).context("hashing custom recipe")?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn check_title(title: &str) -> Result<()> {
    if title.trim().is_empty() || title.len() > MAX_TITLE_LEN {
        bail!("title must be 1 to 160 bytes");
    }
    Ok(())
}

fn check_source(source: &str) -> Result<()> {
    if source.is_empty() || source.len() > MAX_SOURCE {
        bail!("source must be 1 to {MAX_SOURCE} bytes");
    }
    Ok(())
}

fn check_services(services: &[String]) -> Result<()> {
    if services.len() > MAX_SERVICES
        || services
            .iter()
            .any(|name| name.is_empty() || name.len() > MAX_SERVICE_LEN)
    {
        bail!("services: at most 16 names, each 1 to 80 bytes (All accepted)");
    }
    Ok(())
}

pub(super) fn check_id(id: &str) -> Result<()> {
    let ok = (1..=MAX_ID_LEN).contains(&id.len())
        && id.chars().next().is_some_and(|ch| ch.is_ascii_alphabetic())
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_');
    if ok {
        Ok(())
    } else {
        bail!("custom recipe id {id:?} is not a safe token")
    }
}

pub(super) fn is_hash(value: &str) -> bool {
    value.len() == HASH_LEN
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn deny_secret_fields(definition: &RecipeDefinition) -> Result<()> {
    deny_listed(
        &[
            definition.id.as_str(),
            definition.title.as_str(),
            definition.source.as_str(),
        ],
        &definition.services,
    )
}

fn deny_listed(fields: &[&str], services: &[String]) -> Result<()> {
    for text in fields
        .iter()
        .copied()
        .chain(services.iter().map(String::as_str))
    {
        deny_literal(text)?;
    }
    Ok(())
}

fn deny_literal(text: &str) -> Result<()> {
    // A convenience guard for obvious literals, not a secret-detection or
    // exfiltration guarantee. Runtime capability separation is the boundary.
    let lower = text.to_ascii_lowercase();
    if SECRET_MARKERS
        .into_iter()
        .any(|marker| lower.contains(marker))
    {
        bail!("custom recipe must not contain secrets");
    }
    Ok(())
}
