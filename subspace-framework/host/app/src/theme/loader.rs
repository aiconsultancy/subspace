//! Register only this product's embedded themes and product-local custom files.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use gpui_kit::component::{ThemeRegistry, ThemeSet};
use gpui_kit::{App, Global};
use serde_json::Value;
use subspace_dashboard::CurrentProduct;
use subspace_product::ValidatedProduct;

use super::{GPUI_KIT_THEMES, SUBSPACE_THEMES, ThemeDocument};

pub(super) struct ThemeChoices {
    pub(super) documents: BTreeMap<String, ThemeDocument>,
}

impl Global for ThemeChoices {}

fn bundled_theme(id: &str) -> Result<&'static str, String> {
    SUBSPACE_THEMES
        .iter()
        .chain(GPUI_KIT_THEMES)
        .find(|(name, _)| *name == id)
        .map(|(_, content)| *content)
        .ok_or_else(|| format!("product theme is not embedded: {id}"))
}

fn variant_names(id: &str) -> Result<Vec<String>, String> {
    let set: ThemeSet =
        serde_json::from_str(bundled_theme(id)?).map_err(|error| error.to_string())?;
    if set.themes.is_empty() {
        return Err(format!("product theme has no variants: {id}"));
    }
    Ok(set
        .themes
        .into_iter()
        .map(|theme| theme.name.to_string())
        .collect())
}

/// The product theme id of the embedded file that holds the variant `name`,
/// or `custom`. Custom theme names can contain private data, so only their
/// category leaves the app.
pub(crate) fn bundled_theme_id(name: &str) -> &'static str {
    SUBSPACE_THEMES
        .iter()
        .chain(GPUI_KIT_THEMES)
        .copied()
        .find(|(_, content)| contains_theme(content, name))
        .map_or("custom", |(id, _)| id)
}

fn contains_theme(content: &str, name: &str) -> bool {
    serde_json::from_str::<Value>(content)
        .ok()
        .and_then(|value| value.get("themes").and_then(Value::as_array).cloned())
        .is_some_and(|themes| {
            themes
                .iter()
                .any(|theme| theme.get("name").and_then(Value::as_str) == Some(name))
        })
}

/// Every declared document must be embedded, and one of them must contain
/// the product's default variant.
pub(crate) fn validate_product_themes(product: &ValidatedProduct) -> Result<(), String> {
    let names = product
        .themes
        .iter()
        .map(|id| variant_names(id.as_str()))
        .collect::<Result<Vec<_>, _>>()?
        .concat();
    require_default_variant(&names, &product.default_theme)
}

fn require_default_variant(names: &[String], default: &str) -> Result<(), String> {
    if names.iter().any(|name| name == default) {
        Ok(())
    } else {
        Err(format!(
            "product default theme is not in its theme catalogue: {default}"
        ))
    }
}

pub(super) fn default_theme(cx: &App) -> String {
    cx.global::<CurrentProduct>()
        .0
        .product
        .default_theme
        .clone()
}

pub(crate) fn validate_custom_theme(content: &str) -> Result<(), String> {
    let documents = theme_documents(content)?;
    let reserved = reserved_theme_names()?;
    for name in documents.keys() {
        if reserved.contains(&name.to_lowercase()) {
            return Err(format!("reserved theme name: {name}"));
        }
    }
    Ok(())
}

fn reserved_theme_names() -> Result<BTreeSet<String>, String> {
    SUBSPACE_THEMES
        .iter()
        .chain(GPUI_KIT_THEMES)
        .map(|(_, content)| *content)
        .map(|content| serde_json::from_str::<ThemeSet>(content).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()
        .map(|sets| {
            sets.into_iter()
                .flat_map(|set| {
                    set.themes
                        .into_iter()
                        .map(|theme| theme.name.to_lowercase())
                })
                .collect()
        })
}

fn theme_documents(content: &str) -> Result<BTreeMap<String, ThemeDocument>, String> {
    let document = ThemeDocument::parse(content)?;
    let names = document.names();
    validate_catalog_names(&names, &[])?;
    names
        .into_iter()
        .map(|name| {
            let variant = document.variant(&name)?;
            variant.config()?;
            variant.metadata()?;
            Ok((name, variant))
        })
        .collect()
}

fn validate_catalog_names(names: &[String], existing: &[String]) -> Result<(), String> {
    if names.is_empty() {
        return Err("theme document has no variants".to_owned());
    }
    let mut occupied: BTreeSet<_> = existing.iter().map(|name| name.to_lowercase()).collect();
    for name in names {
        if name.trim().is_empty() {
            return Err("theme name must not be blank".to_owned());
        }
        if !occupied.insert(name.to_lowercase()) {
            return Err(format!("duplicate theme name: {name}"));
        }
    }
    Ok(())
}

fn file_warning(path: &Path, error: &dyn std::fmt::Display) -> String {
    format!("{}: {error}", path.display())
}

pub(super) fn load_registered_themes(cx: &mut App) -> Vec<String> {
    let mut warnings = Vec::new();
    let selected = cx.global::<CurrentProduct>().0.product.themes.clone();
    cx.set_global(ThemeChoices {
        documents: BTreeMap::new(),
    });
    for id in selected {
        match bundled_theme(id.as_str()) {
            Ok(content) => load_theme(content, id.as_str(), &mut warnings, cx),
            Err(error) => warnings.push(error),
        }
    }
    if let Some(directory) = crate::product::gui_config_file(cx).parent() {
        load_custom_themes(&directory.join("themes"), &mut warnings, cx);
    }
    warnings
}

fn custom_theme_files(directory: &Path) -> Result<Vec<std::path::PathBuf>, std::io::Error> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut files = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("json") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn load_custom_themes(directory: &Path, warnings: &mut Vec<String>, cx: &mut App) {
    let files = match custom_theme_files(directory) {
        Ok(files) => files,
        Err(error) => {
            warnings.push(file_warning(directory, &error));
            return;
        }
    };
    for path in files {
        match super::storage::read_theme(&path).and_then(|content| {
            validate_custom_theme(&content)?;
            Ok(content)
        }) {
            Ok(content) => load_theme(&content, &path.display().to_string(), warnings, cx),
            Err(error) => warnings.push(file_warning(&path, &error)),
        }
    }
}

fn load_theme(content: &str, label: &str, warnings: &mut Vec<String>, cx: &mut App) {
    let result = theme_documents(content).and_then(|documents| {
        let existing = cx
            .global::<ThemeChoices>()
            .documents
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let names = documents.keys().cloned().collect::<Vec<_>>();
        validate_catalog_names(&names, &existing)?;
        ThemeRegistry::global_mut(cx)
            .load_themes_from_str(content)
            .map_err(|error| error.to_string())?;
        Ok(documents)
    });
    match result {
        Ok(documents) => cx.global_mut::<ThemeChoices>().documents.extend(documents),
        Err(error) => warnings.push(format!("{label}: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::super::THEME_JSON;
    use super::{
        GPUI_KIT_THEMES, bundled_theme, custom_theme_files, file_warning, theme_documents,
        validate_catalog_names, validate_custom_theme, validate_product_themes, variant_names,
    };
    use crate::theme::THEME_NAME;

    #[test]
    fn known_products_default_to_cupertino_dark_from_their_embedded_documents() {
        for product in subspace_product::fixture::products().unwrap() {
            validate_product_themes(&product).unwrap();
            assert_eq!(product.default_theme, "Cupertino Dark");
        }
        assert!(bundled_theme("unknown").is_err());
        assert!(variant_names("unknown").is_err());
        assert_eq!(
            variant_names("cupertino").unwrap(),
            ["Cupertino Light", "Cupertino Dark"]
        );
        assert!(bundled_theme("shell").is_err());
        assert!(bundled_theme("subspace-shell-dark").is_err());
    }

    #[test]
    fn a_default_variant_outside_the_declared_documents_fails_validation() {
        let mut product = subspace_product::fixture::products().unwrap().remove(0);
        product.themes.retain(|id| id.as_str() == "one-dark");
        let error = validate_product_themes(&product).unwrap_err();
        assert!(error.contains("Cupertino Dark"), "{error}");
        product.default_theme = THEME_NAME.into();
        assert!(validate_product_themes(&product).is_ok());
    }

    #[test]
    fn all_bundled_variant_names_are_reserved_case_insensitively() {
        for content in [THEME_JSON]
            .into_iter()
            .chain(GPUI_KIT_THEMES.iter().map(|(_, content)| *content))
        {
            let value: serde_json::Value = serde_json::from_str(content).unwrap();
            for variant in value["themes"].as_array().unwrap() {
                let name = variant["name"].as_str().unwrap().to_uppercase();
                let custom = THEME_JSON.replace(THEME_NAME, &name);
                let error = validate_custom_theme(&custom).unwrap_err();
                assert!(error.contains("reserved theme name"), "{name}: {error}");
            }
        }
    }

    #[test]
    fn catalog_rejects_entire_batch_when_any_name_collides() {
        let existing = vec!["My Theme".to_owned()];
        let names = vec!["New Theme".to_owned(), "my theme".to_owned()];
        assert!(validate_catalog_names(&names, &existing).is_err());
        assert_eq!(existing, ["My Theme"]);
        assert!(validate_catalog_names(&["Unique".to_owned()], &existing).is_ok());
        assert!(validate_catalog_names(&["Same".to_owned(), "same".to_owned()], &[]).is_err());
        assert!(validate_catalog_names(&["  ".to_owned()], &[]).is_err());
        assert!(validate_catalog_names(&[], &[]).is_err());
    }

    #[test]
    fn catalog_keeps_each_original_variant_document() {
        let documents = theme_documents(
            GPUI_KIT_THEMES
                .iter()
                .find(|(name, _)| *name == "solarized")
                .unwrap()
                .1,
        )
        .unwrap();
        assert_eq!(documents.len(), 2);
        for (name, document) in documents {
            assert_eq!(document.names(), vec![name]);
            assert!(document.config().is_ok());
            assert!(document.metadata().is_ok());
        }
    }

    #[test]
    fn custom_documents_reject_duplicate_and_blank_names() {
        let mut value: serde_json::Value = serde_json::from_str(THEME_JSON).unwrap();
        value["themes"][0]["name"] = "Custom".into();
        assert!(validate_custom_theme(&value.to_string()).is_ok());
        let duplicate = value["themes"][0].clone();
        value["themes"].as_array_mut().unwrap().push(duplicate);
        assert!(validate_custom_theme(&value.to_string()).is_err());
        value["themes"].as_array_mut().unwrap().pop();
        value["themes"][0]["name"] = " ".into();
        assert!(validate_custom_theme(&value.to_string()).is_err());
        assert!(validate_custom_theme("{malformed").is_err());
    }

    #[test]
    fn custom_files_are_sorted_and_missing_directories_are_optional() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            custom_theme_files(&dir.path().join("missing"))
                .unwrap()
                .is_empty()
        );
        for name in ["b.json", "a.json", "ignored.txt"] {
            std::fs::write(dir.path().join(name), "{}").unwrap();
        }
        assert_eq!(
            custom_theme_files(dir.path()).unwrap(),
            [dir.path().join("a.json"), dir.path().join("b.json")]
        );
        assert!(custom_theme_files(&dir.path().join("a.json")).is_err());
        assert_eq!(
            file_warning(&dir.path().join("a.json"), &"invalid"),
            format!("{}: invalid", dir.path().join("a.json").display())
        );
    }
}
