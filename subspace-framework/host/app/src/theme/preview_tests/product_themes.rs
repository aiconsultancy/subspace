//! A product's own theme documents, installed as the desktop start installs
//! them: listed for Settings, reported by their own id, and reserved.

use std::path::Path;
use std::sync::Arc;

use gpui_kit::{App, TestAppContext};
use subspace_dashboard::CurrentProduct;
use subspace_dashboard::tokens::ThemeId;

use super::{init_app, product_context};
use crate::ProductThemes;
use crate::theme::{
    THEME_JSON, THEME_NAME, bundled_theme_id, initialize, product_themes, theme_names,
};

const VARIANT: &str = "Example Dark";

fn documents() -> ProductThemes {
    let json: &'static str = Box::leak(THEME_JSON.replace(THEME_NAME, VARIANT).into_boxed_str());
    Box::leak(Box::new([("example-dark", json)]))
}

/// A product that declares `example-dark` and starts in its variant.
fn start_with_product_themes(root: &Path, cx: &mut App) {
    init_app(cx, root, root.join("state.json"));
    let mut context = product_context(root);
    let id = subspace_product::parse_identity("example-dark").unwrap();
    context.product.themes.push(id);
    context.product.default_theme = VARIANT.into();
    cx.set_global(CurrentProduct(Arc::new(context)));
    crate::test_support::install_product_themes(documents(), cx).unwrap();
}

#[gpui_kit::test]
fn a_product_variant_is_listed_and_reported_by_its_documents_id(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        start_with_product_themes(root.path(), cx);
        let warnings = initialize(cx);
        assert!(warnings.is_empty(), "{warnings:?}");
        let names = theme_names(cx);
        assert!(names.iter().any(|name| name == VARIANT), "{names:?}");
        assert!(
            names.iter().any(|name| name == "Cupertino Dark"),
            "{names:?}"
        );
        assert_eq!(cx.global::<ThemeId>().0, "example-dark");
        assert_eq!(
            bundled_theme_id(VARIANT, product_themes(cx)),
            "example-dark"
        );
    });
}

#[gpui_kit::test]
fn a_custom_theme_file_cannot_reuse_a_product_variant_name(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let themes = root.path().join("themes");
    std::fs::create_dir_all(&themes).unwrap();
    let custom = THEME_JSON.replace(THEME_NAME, &VARIANT.to_uppercase());
    std::fs::write(themes.join("custom.json"), custom).unwrap();
    cx.update(|cx| {
        start_with_product_themes(root.path(), cx);
        let warnings = initialize(cx);
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("reserved theme name")),
            "{warnings:?}"
        );
        let names = theme_names(cx);
        assert!(names.iter().any(|name| name == VARIANT), "{names:?}");
        assert!(!names.iter().any(|name| name == &VARIANT.to_uppercase()));
    });
}

#[gpui_kit::test]
fn the_harness_refuses_product_documents_startup_would_refuse(cx: &mut TestAppContext) {
    let reused: ProductThemes = &[("example-dark", THEME_JSON)];
    cx.update(|cx| {
        let error = crate::test_support::install_product_themes(reused, cx).unwrap_err();
        assert!(error.contains(THEME_NAME), "{error}");
        assert!(product_themes(cx).is_empty());
    });
}
