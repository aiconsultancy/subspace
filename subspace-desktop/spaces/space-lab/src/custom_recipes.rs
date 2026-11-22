//! Versioned custom recipe storage for the disposable workbench spike.
//! Pure storage: no execution, network, or host-policy grants.

mod definition;
mod storage;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomRecipe {
    pub id: String,
    pub title: String,
    pub environment: String,
    pub source: String,
    pub services: Vec<String>,
    pub network: bool,
    pub hash: String,
}

#[derive(Clone, Debug)]
pub struct RecipeDefinition {
    pub id: String,
    pub title: String,
    pub environment: String,
    pub source: String,
    pub services: Vec<String>,
    pub network: bool,
}

impl CustomRecipe {
    pub fn reference(&self) -> String {
        format!("custom:{}:{}", self.id, self.hash)
    }
}

pub fn register(root: &Path, definition: RecipeDefinition) -> Result<CustomRecipe> {
    definition.validate()?;
    let hash = definition.fingerprint()?;
    let recipe = definition.into_recipe(hash);
    storage::store(root, &recipe)?;
    Ok(recipe)
}

pub fn list(root: &Path) -> Result<Vec<CustomRecipe>> {
    storage::list(root)
}

pub fn load(root: &Path, reference: &str) -> Result<CustomRecipe> {
    let (id, hash) = parse_reference(reference)?;
    storage::load(root, id, hash)
}

fn parse_reference(reference: &str) -> Result<(&str, &str)> {
    let Some(rest) = reference.strip_prefix("custom:") else {
        bail!("reference must start with custom:")
    };
    let Some((id, hash)) = rest.rsplit_once(':') else {
        bail!("reference must be custom:<id>:<hash>")
    };
    definition::check_id(id)?;
    if !definition::is_hash(hash) {
        bail!("reference hash must be 64 lowercase hex characters")
    }
    Ok((id, hash))
}

#[cfg(test)]
mod tests {
    use super::{RecipeDefinition, list, load, register};
    use std::fs;
    use std::path::Path;

    const FIXED_HASH: &str = "90ef6c1d6fd24e80088182e0ce7fc7356f8fb15b0914c71e18236e5fd3bc14d2";

    fn demo() -> RecipeDefinition {
        RecipeDefinition {
            id: "demo".into(),
            title: "Demo recipe".into(),
            environment: "python-data".into(),
            source: "print(1)".into(),
            services: vec!["Freight".into()],
            network: false,
        }
    }

    fn register_demo(root: &Path) -> super::CustomRecipe {
        register(root, demo()).expect("demo recipe should register")
    }

    fn revision_path(root: &Path, recipe: &super::CustomRecipe) -> std::path::PathBuf {
        root.join("custom-recipes")
            .join(&recipe.id)
            .join(format!("{}.json", recipe.hash))
    }

    #[test]
    fn fingerprint_is_stable_for_hash_body_field_order() {
        let directory = tempfile::tempdir().expect("temporary test directory");
        let recipe = register_demo(directory.path());
        assert_eq!(recipe.hash, FIXED_HASH);
        assert_eq!(recipe.reference(), format!("custom:demo:{FIXED_HASH}"));
    }

    #[test]
    fn register_is_idempotent_and_lists_the_stored_revision() {
        let directory = tempfile::tempdir().expect("temporary test directory");
        let first = register_demo(directory.path());
        let second = register_demo(directory.path());
        assert_eq!(first.hash, second.hash);
        let listed = list(directory.path()).expect("list stored recipes");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].hash, FIXED_HASH);
        let loaded = load(directory.path(), &first.reference()).expect("load by reference");
        assert_eq!(loaded.source, "print(1)");
        assert!(!loaded.source.to_ascii_lowercase().contains("password="));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_recipe_path_is_rejected() {
        let directory = tempfile::tempdir().expect("temporary test directory");
        let recipe = register_demo(directory.path());
        let real = revision_path(directory.path(), &recipe);
        let link_root = directory.path().join("linked");
        fs::create_dir_all(link_root.join("custom-recipes").join(&recipe.id)).expect("link parent");
        std::os::unix::fs::symlink(real, revision_path(&link_root, &recipe)).expect("symlink");
        let error = load(link_root.as_path(), &recipe.reference()).expect_err("symlink");
        assert!(format!("{error:#}").contains("symlink rejected"));
    }

    #[test]
    fn tampered_bytes_fail_hash_verification() {
        let directory = tempfile::tempdir().expect("temporary test directory");
        let recipe = register_demo(directory.path());
        let path = revision_path(directory.path(), &recipe);
        let mut record: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("stored recipe"))
                .expect("valid stored JSON");
        record["source"] = serde_json::json!("print(2)");
        fs::write(&path, serde_json::to_vec(&record).expect("tampered JSON")).expect("overwrite");
        let error = load(directory.path(), &recipe.reference()).expect_err("tamper");
        assert!(format!("{error:#}").contains("hash does not match"));
    }

    #[test]
    fn secret_literals_and_empty_title_are_rejected() {
        let directory = tempfile::tempdir().expect("temporary test directory");
        let mut secret = demo();
        secret.source = "token = \"password=demo\"".into();
        let secret_error = register(directory.path(), secret).expect_err("secret");
        assert!(format!("{secret_error:#}").contains("must not contain secrets"));
        let mut empty = demo();
        empty.title = "   ".into();
        let title_error = register(directory.path(), empty).expect_err("title");
        assert!(format!("{title_error:#}").contains("title must be 1 to 160 bytes"));
    }
}
