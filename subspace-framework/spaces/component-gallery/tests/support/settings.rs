use std::collections::HashSet;
use std::path::PathBuf;

use serde_json::Value;

pub(crate) const BODY: &str = "Inter";
pub(crate) const MONO: &str = "JetBrainsMono Nerd Font";
// Capture raster boundaries, not component layout or spacing overrides.
pub(crate) const WIDTH: u32 = 800;
pub(crate) const HEIGHT: u32 = 600;
pub(crate) const MAX_HEIGHT: u32 = 4096;
pub(crate) const SCALE: u32 = 2;
pub(crate) const DEFAULT_CASES: &[(&str, &str)] = &[
    ("action::action_button", "action-button"),
    ("fields::input", "input"),
    ("feedback::error_box", "error-box"),
];
// Bound each process to 24 readbacks; the coordinator supplies successive batches.
pub(crate) const MAX_CASES: usize = 12;
pub(crate) const THEMES: &[(&str, &str, &str)] = &[
    ("one-dark.json", "Subspace One Dark", "one-dark"),
    ("cupertino.json", "Cupertino Dark", "cupertino-dark"),
];

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CaptureCase {
    pub(crate) item: String,
    pub(crate) slug: String,
}

pub(crate) fn selected_cases() -> Vec<CaptureCase> {
    let json = std::env::var("SUBSPACE_GALLERY_CASES");
    match json {
        Ok(json) => parse_cases(&json).expect("valid SUBSPACE_GALLERY_CASES batch"),
        Err(std::env::VarError::NotPresent) => DEFAULT_CASES
            .iter()
            .map(|&(item, slug)| CaptureCase {
                item: item.into(),
                slug: slug.into(),
            })
            .collect(),
        Err(error) => panic!("SUBSPACE_GALLERY_CASES must be Unicode JSON: {error}"),
    }
}

pub(crate) fn parse_cases(json: &str) -> Result<Vec<CaptureCase>, String> {
    let value: Value = serde_json::from_str(json).map_err(|error| error.to_string())?;
    let entries = value.as_array().ok_or("expected a JSON array")?;
    if entries.is_empty() || entries.len() > MAX_CASES {
        return Err(format!("expected 1..={MAX_CASES} cards per batch"));
    }
    let mut items = HashSet::new();
    let mut slugs = HashSet::new();
    let mut cases = Vec::new();
    for entry in entries {
        let case = parse_case(entry)?;
        if !items.insert(case.item.clone()) || !slugs.insert(case.slug.clone()) {
            return Err("duplicate item or slug in batch".into());
        }
        cases.push(case);
    }
    Ok(cases)
}

fn parse_case(entry: &Value) -> Result<CaptureCase, String> {
    let object = entry.as_object().ok_or("expected an item/slug object")?;
    let item = entry["item"].as_str().ok_or("item must be a string")?;
    let slug = entry["slug"].as_str().ok_or("slug must be a string")?;
    if object.len() != 2 || item.is_empty() || item.trim() != item || item.len() > 256 {
        return Err("expected only an exact canonical item and its web slug".into());
    }
    if !valid_slug(slug) {
        return Err("slug must contain lowercase ASCII letters, digits and single hyphens".into());
    }
    Ok(CaptureCase {
        item: item.into(),
        slug: slug.into(),
    })
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 160
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && !slug.contains("--")
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

pub(crate) fn absolute_directory(variable: &str) -> PathBuf {
    let path = PathBuf::from(std::env::var_os(variable).unwrap_or_else(|| {
        panic!("set {variable} explicitly; this test never reads application configuration")
    }));
    assert!(path.is_absolute(), "{variable} must be an absolute path");
    path
}

pub(crate) fn bounded_height(bottom_with_inset: f32) -> u32 {
    assert!(bottom_with_inset.is_finite() && bottom_with_inset > 0.);
    assert!(
        bottom_with_inset <= MAX_HEIGHT as f32,
        "natural card height {bottom_with_inset} exceeds the {MAX_HEIGHT}px capture ceiling"
    );
    (bottom_with_inset.ceil() as u32).max(HEIGHT)
}

pub(crate) fn selected_minimum_height() -> u32 {
    match std::env::var("SUBSPACE_GALLERY_MIN_HEIGHT") {
        Ok(value) => parse_minimum_height(&value).expect("valid explicit visual-review viewport"),
        Err(std::env::VarError::NotPresent) => HEIGHT,
        Err(error) => panic!("SUBSPACE_GALLERY_MIN_HEIGHT must be Unicode: {error}"),
    }
}

pub(crate) fn parse_minimum_height(value: &str) -> Result<u32, String> {
    let height = value.parse::<u32>().map_err(|error| error.to_string())?;
    if !(HEIGHT..=MAX_HEIGHT).contains(&height) {
        return Err(format!("minimum height must be {HEIGHT}..={MAX_HEIGHT}"));
    }
    Ok(height)
}
