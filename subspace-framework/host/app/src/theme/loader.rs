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

/// Theme documents a product embeds beside the Framework's catalogue, as
/// `(id, json)`.
pub type ProductThemes = &'static [(&'static str, &'static str)];

/// The theme id Report issue gives every theme that is not embedded.
const CUSTOM_THEME_ID: &str = "custom";

/// The themes GPUI Kit registers from its own `default-theme.json` when it
/// starts, sorted. Its registry never replaces a registered name, so no
/// document may use one; a test reads them from the registry.
const KIT_PRELOADED_THEMES: [&str; 2] = ["Default Dark", "Default Light"];

/// The running product's validated theme documents.
struct ProductThemeDocuments(ProductThemes);

impl Global for ProductThemeDocuments {}

/// Installs the product's theme documents, already validated, for the theme
/// loader to read. Install them before the theme starts.
pub(crate) fn install_product_themes(themes: ProductThemes, cx: &mut App) {
    cx.set_global(ProductThemeDocuments(themes));
}

/// The installed product theme documents; none until they are installed.
pub(crate) fn product_themes(cx: &App) -> ProductThemes {
    cx.try_global::<ProductThemeDocuments>()
        .map_or(&[], |documents| documents.0)
}

/// Every embedded document: the product's, then Subspace's, then GPUI Kit's.
fn catalogue(product: ProductThemes) -> impl Iterator<Item = (&'static str, &'static str)> {
    product
        .iter()
        .chain(SUBSPACE_THEMES)
        .chain(GPUI_KIT_THEMES)
        .copied()
}

fn bundled_theme(id: &str, product: ProductThemes) -> Result<&'static str, String> {
    catalogue(product)
        .find(|(name, _)| *name == id)
        .map(|(_, content)| content)
        .ok_or_else(|| format!("product theme is not embedded: {id}"))
}

/// Each product document must parse as a theme set with at least one
/// variant, both as the theme editor reads it and as GPUI Kit does, which
/// refuses a repeated field. Its id must not be `custom` or repeat a
/// Framework document's id or another product document's id, and no variant
/// name may repeat, ignoring case, a Framework or GPUI Kit variant or one in
/// another product document.
pub(crate) fn validate_product_theme_documents(product: ProductThemes) -> Result<(), String> {
    let documents = product
        .iter()
        .map(|(id, content)| product_document(id, content))
        .collect::<Result<Vec<_>, _>>()?;
    require_unique_product_names(&documents, reserved_theme_names(&[])?)
}

/// `id` with its variant names, once `content` parses for every reader.
fn product_document(
    id: &'static str,
    content: &str,
) -> Result<(&'static str, Vec<String>), String> {
    serde_json::from_str::<ThemeSet>(content)
        .map_err(|error| error.to_string())
        .and_then(|_| theme_documents(content))
        .map(|variants| (id, variants.into_keys().collect()))
        .map_err(|error| format!("product theme {id}: {error}"))
}

/// `documents` are `(id, variant names)`; `reserved` holds the lower-case
/// Framework and GPUI Kit variant names.
fn require_unique_product_names(
    documents: &[(&str, Vec<String>)],
    mut reserved: BTreeSet<String>,
) -> Result<(), String> {
    let mut ids = BTreeSet::new();
    for (id, names) in documents {
        if *id == CUSTOM_THEME_ID {
            return Err(format!("product theme id is reserved: {id}"));
        }
        let reused = SUBSPACE_THEMES
            .iter()
            .chain(GPUI_KIT_THEMES)
            .any(|(name, _)| name == id);
        if reused || !ids.insert(*id) {
            return Err(format!("product theme id is already in use: {id}"));
        }
        if let Some(name) = names
            .iter()
            .find(|name| !reserved.insert(name.to_lowercase()))
        {
            return Err(format!(
                "product theme {id}: theme name is already in use: {name}"
            ));
        }
    }
    Ok(())
}

fn variant_names(id: &str, product: ProductThemes) -> Result<Vec<String>, String> {
    let set: ThemeSet = serde_json::from_str(bundled_theme(id, product)?)
        .map_err(|error| format!("product theme {id}: {error}"))?;
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
pub(crate) fn bundled_theme_id(name: &str, product: ProductThemes) -> &'static str {
    catalogue(product)
        .find(|(_, content)| contains_theme(content, name))
        .map_or(CUSTOM_THEME_ID, |(id, _)| id)
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
pub(crate) fn validate_product_themes(
    product: &ValidatedProduct,
    documents: ProductThemes,
) -> Result<(), String> {
    validate_product_theme_documents(documents)?;
    require_catalogue_variant(product, "default theme", &product.default_theme, documents)
}

/// `name`, the product's `role` theme, must be a variant of one of its
/// declared documents.
pub(crate) fn require_catalogue_variant(
    product: &ValidatedProduct,
    role: &str,
    name: &str,
    documents: ProductThemes,
) -> Result<(), String> {
    let names = product
        .themes
        .iter()
        .map(|id| variant_names(id.as_str(), documents))
        .collect::<Result<Vec<_>, _>>()?
        .concat();
    require_variant(&names, role, name)
}

fn require_variant(names: &[String], role: &str, name: &str) -> Result<(), String> {
    if names.iter().any(|candidate| candidate == name) {
        Ok(())
    } else {
        Err(format!(
            "product {role} is not in its theme catalogue: {name}"
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

pub(crate) fn validate_custom_theme(content: &str, product: ProductThemes) -> Result<(), String> {
    let documents = theme_documents(content)?;
    let reserved = reserved_theme_names(product)?;
    for name in documents.keys() {
        if reserved.contains(&name.to_lowercase()) {
            return Err(format!("reserved theme name: {name}"));
        }
    }
    Ok(())
}

fn reserved_theme_names(product: ProductThemes) -> Result<BTreeSet<String>, String> {
    catalogue(product)
        .map(|(_, content)| content)
        .map(|content| serde_json::from_str::<ThemeSet>(content).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()
        .map(|sets| {
            sets.into_iter()
                .flat_map(|set| {
                    set.themes
                        .into_iter()
                        .map(|theme| theme.name.to_lowercase())
                })
                .chain(KIT_PRELOADED_THEMES.map(str::to_lowercase))
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
        match bundled_theme(id.as_str(), product_themes(cx)) {
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
    let product = product_themes(cx);
    for path in files {
        match super::storage::read_theme(&path).and_then(|content| {
            validate_custom_theme(&content, product)?;
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
    use gpui_kit::TestAppContext;
    use gpui_kit::component::ThemeRegistry;

    use super::{
        CUSTOM_THEME_ID, GPUI_KIT_THEMES, KIT_PRELOADED_THEMES, ProductThemes, bundled_theme,
        custom_theme_files, file_warning, reserved_theme_names, theme_documents,
        validate_catalog_names, validate_custom_theme, validate_product_theme_documents,
        validate_product_themes, variant_names,
    };
    use crate::theme::THEME_NAME;

    #[test]
    fn known_products_default_to_cupertino_dark_from_their_embedded_documents() {
        for product in subspace_product::fixture::products().unwrap() {
            validate_product_themes(&product, &[]).unwrap();
            assert_eq!(product.default_theme, "Cupertino Dark");
        }
        assert!(bundled_theme("unknown", &[]).is_err());
        assert!(variant_names("unknown", &[]).is_err());
        assert_eq!(
            variant_names("cupertino", &[]).unwrap(),
            ["Cupertino Light", "Cupertino Dark"]
        );
        assert!(bundled_theme("shell", &[]).is_err());
        assert!(bundled_theme("subspace-shell-dark", &[]).is_err());
    }

    #[test]
    fn a_default_variant_outside_the_declared_documents_fails_validation() {
        let mut product = subspace_product::fixture::products().unwrap().remove(0);
        product.themes.retain(|id| id.as_str() == "one-dark");
        let error = validate_product_themes(&product, &[]).unwrap_err();
        assert!(error.contains("Cupertino Dark"), "{error}");
        product.default_theme = THEME_NAME.into();
        assert!(validate_product_themes(&product, &[]).is_ok());
    }

    fn example_dark() -> String {
        THEME_JSON.replace(THEME_NAME, "Example Dark")
    }

    #[test]
    fn a_product_document_can_be_declared_and_be_the_default() {
        let json: &'static str = Box::leak(example_dark().into_boxed_str());
        let documents: ProductThemes = Box::leak(Box::new([("example-dark", json)]));
        let mut product = subspace_product::fixture::products().unwrap().remove(0);
        assert!(validate_product_themes(&product, documents).is_ok());
        let id = subspace_product::parse_identity("example-dark").unwrap();
        product.themes.push(id);
        product.default_theme = "Example Dark".into();
        assert!(validate_product_themes(&product, documents).is_ok());
        assert_eq!(
            variant_names("example-dark", documents).unwrap(),
            ["Example Dark"]
        );
        assert_eq!(bundled_theme("example-dark", documents).unwrap(), json);
        assert!(validate_product_themes(&product, &[]).is_err());
        let reserved = reserved_theme_names(documents).unwrap();
        assert!(reserved.contains("example dark"));
    }

    #[test]
    fn a_product_document_with_invalid_json_or_a_reused_id_is_rejected() {
        let broken: ProductThemes = &[("example-dark", "{malformed")];
        let error = validate_product_theme_documents(broken).unwrap_err();
        assert!(error.contains("example-dark"), "{error}");
        let reused: ProductThemes = &[("one-dark", THEME_JSON)];
        let error = validate_product_theme_documents(reused).unwrap_err();
        assert!(error.contains("already in use"), "{error}");
        let json: &'static str = Box::leak(example_dark().into_boxed_str());
        let twice: ProductThemes = Box::leak(Box::new([("a", json), ("a", json)]));
        assert!(validate_product_theme_documents(twice).is_err());
    }

    #[test]
    fn a_product_variant_repeating_another_documents_variant_is_rejected() {
        let framework = THEME_JSON.replace(THEME_NAME, "cupertino DARK");
        let framework: &'static str = Box::leak(framework.into_boxed_str());
        let documents: ProductThemes = Box::leak(Box::new([("example-dark", framework)]));
        let error = validate_product_theme_documents(documents).unwrap_err();
        assert!(error.contains("example-dark"), "{error}");
        assert!(error.contains("cupertino DARK"), "{error}");
        let json: &'static str = Box::leak(example_dark().into_boxed_str());
        let again = THEME_JSON.replace(THEME_NAME, "EXAMPLE dark");
        let again: &'static str = Box::leak(again.into_boxed_str());
        let documents: ProductThemes =
            Box::leak(Box::new([("example-dark", json), ("example-more", again)]));
        let error = validate_product_theme_documents(documents).unwrap_err();
        assert!(error.contains("example-more"), "{error}");
        assert!(error.contains("EXAMPLE dark"), "{error}");
    }

    #[test]
    fn an_unlisted_product_document_with_a_duplicate_field_is_rejected() {
        let duplicate = example_dark().replace(
            "\"name\": \"Example Dark\",",
            "\"name\": \"Example Dark\", \"name\": \"Example Dark\",",
        );
        let duplicate: &'static str = Box::leak(duplicate.into_boxed_str());
        let documents: ProductThemes = Box::leak(Box::new([("example-dark", duplicate)]));
        let error = validate_product_theme_documents(documents).unwrap_err();
        assert!(error.contains("example-dark"), "{error}");
        assert!(error.contains("duplicate field"), "{error}");
        // The readers after start-up would refuse the whole catalogue.
        assert!(reserved_theme_names(documents).is_err());
    }

    #[test]
    fn a_product_variant_named_like_a_gpui_kit_default_is_rejected() {
        for name in KIT_PRELOADED_THEMES {
            let json = THEME_JSON.replace(THEME_NAME, &name.to_uppercase());
            let json: &'static str = Box::leak(json.into_boxed_str());
            let documents: ProductThemes = Box::leak(Box::new([("example-dark", json)]));
            let error = validate_product_theme_documents(documents).unwrap_err();
            assert!(error.contains("already in use"), "{name}: {error}");
            let custom = THEME_JSON.replace(THEME_NAME, name);
            let error = validate_custom_theme(&custom, &[]).unwrap_err();
            assert!(error.contains("reserved theme name"), "{name}: {error}");
        }
    }

    /// GPUI Kit registers its own defaults before any document loads and
    /// never replaces a registered name, so each must be reserved.
    #[gpui_kit::test]
    fn the_reserved_kit_defaults_are_the_themes_gpui_kit_preloads(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            let mut preloaded = ThemeRegistry::global(cx)
                .themes()
                .keys()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            preloaded.sort();
            assert_eq!(preloaded, KIT_PRELOADED_THEMES);
        });
    }

    #[test]
    fn a_product_document_cannot_use_the_custom_theme_id() {
        let json: &'static str = Box::leak(example_dark().into_boxed_str());
        let documents: ProductThemes = Box::leak(Box::new([(CUSTOM_THEME_ID, json)]));
        let error = validate_product_theme_documents(documents).unwrap_err();
        assert!(error.contains("reserved"), "{error}");
        assert!(error.contains(CUSTOM_THEME_ID), "{error}");
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
                let error = validate_custom_theme(&custom, &[]).unwrap_err();
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
        assert!(validate_custom_theme(&value.to_string(), &[]).is_ok());
        let duplicate = value["themes"][0].clone();
        value["themes"].as_array_mut().unwrap().push(duplicate);
        assert!(validate_custom_theme(&value.to_string(), &[]).is_err());
        value["themes"].as_array_mut().unwrap().pop();
        value["themes"][0]["name"] = " ".into();
        assert!(validate_custom_theme(&value.to_string(), &[]).is_err());
        assert!(validate_custom_theme("{malformed", &[]).is_err());
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
