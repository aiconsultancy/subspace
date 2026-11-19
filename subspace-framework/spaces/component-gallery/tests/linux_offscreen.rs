//! Opt-in Linux GPU proof of actual catalogue cards, with no application launch.
#![cfg(all(target_os = "linux", feature = "test-support"))]

mod support;

use std::sync::Arc;

use gpui_kit::HeadlessAppContext;
use gpui_kit::component::Theme;
use serde_json::json;
use subspace_dashboard::prefs;

use support::manifest::write_manifest;
use support::render::capture_case;
use support::settings::{
    BODY, DEFAULT_CASES, HEIGHT, MAX_CASES, MAX_HEIGHT, MONO, THEMES, absolute_directory,
    bounded_height, parse_cases, parse_minimum_height, selected_cases, selected_minimum_height,
};
use support::themes::{apply_theme, load_theme, require_fonts};

/// Owns its TestPlatform context because the default GPUI test macro has no GPU.
#[test]
#[ignore = "needs Linux GPU or Vulkan software driver, installed fonts and explicit fixture/output paths"]
fn linux_offscreen_gallery_pngs() {
    let cases = selected_cases();
    let minimum_height = selected_minimum_height();
    let fixtures = absolute_directory("SUBSPACE_GALLERY_THEME_FIXTURES");
    let output = absolute_directory("SUBSPACE_GALLERY_PNG_OUTPUT");
    std::fs::create_dir(&output).expect("output must be a new directory, to avoid stale evidence");
    let platform = gpui_kit::platform::current_platform(true);
    let mut cx = HeadlessAppContext::with_platform(
        platform.text_system(),
        Arc::new(gpui_kit::assets::Assets),
        gpui_kit::platform::current_headless_renderer,
    );
    require_fonts(&cx);
    cx.update(|cx| {
        // Initialise with the approved installed families before GPUI probes its
        // virtual system font, which this minimal Linux host does not provide.
        let mut theme = Theme::default();
        theme.font_family = BODY.into();
        theme.mono_font_family = MONO.into();
        cx.set_global(theme);
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
    let mut records = Vec::new();
    for &(file, theme_name, slug) in THEMES {
        let fixture = load_theme(&fixtures.join(file), theme_name);
        cx.update(|cx| apply_theme(&fixture, cx));
        for case in &cases {
            let path = output.join(format!("{slug}-{}.png", case.slug));
            records.push(capture_case(&mut cx, case, &path, minimum_height));
        }
    }
    assert_eq!(records.len(), THEMES.len() * cases.len());
    write_manifest(&output, records);
    eprintln!("Linux offscreen gallery PNGs: {}", output.display());
}

#[test]
fn case_batch_preserves_exact_names_and_web_slugs() {
    let cases = parse_cases(
        r#"[{"item":"gpui_kit::component::button::Button","slug":"kit-basic-gpui-kit-component-button-button"}]"#,
    )
    .unwrap();
    assert_eq!(cases[0].item, "gpui_kit::component::button::Button");
    assert_eq!(cases[0].slug, "kit-basic-gpui-kit-component-button-button");
    let defaults: Vec<_> = DEFAULT_CASES
        .iter()
        .map(|(item, slug)| json!({ "item": item, "slug": slug }))
        .collect();
    assert_eq!(parse_cases(&json!(defaults).to_string()).unwrap().len(), 3);
}

#[test]
fn case_batch_rejects_empty_oversized_and_duplicate_selections() {
    let full: Vec<_> = (0..MAX_CASES)
        .map(|ix| json!({ "item": format!("card::{ix}"), "slug": format!("card-{ix}") }))
        .collect();
    assert_eq!(
        parse_cases(&json!(full).to_string()).unwrap().len(),
        MAX_CASES
    );
    let case = json!({ "item": "fields::input", "slug": "input" });
    assert!(parse_cases("[]").is_err());
    assert!(parse_cases(&json!(vec![case.clone(); MAX_CASES + 1]).to_string()).is_err());
    assert!(parse_cases(&json!([case.clone(), case]).to_string()).is_err());
    assert!(parse_cases(r#"[{"item":"same","slug":"a"},{"item":"same","slug":"b"}]"#).is_err());
    assert!(parse_cases(r#"[{"item":"a","slug":"same"},{"item":"b","slug":"same"}]"#).is_err());
}

#[test]
fn case_batch_rejects_paths_and_extra_settings() {
    for slug in [
        "../input",
        "\\input",
        "/input",
        "input.png",
        "Input",
        "-input",
        "input-",
        "a--b",
        "",
    ] {
        assert!(
            parse_cases(&json!([{ "item": "fields::input", "slug": slug }]).to_string()).is_err()
        );
    }
    for invalid in [
        "[",
        r#"{}"#,
        r#"["fields::input"]"#,
        r#"[{"item":" fields::input","slug":"input"}]"#,
        r#"[{"item":"","slug":"input"}]"#,
        r#"[{"item":"fields::input","slug":"input","output":"/tmp"}]"#,
        r#"[{"item":1,"slug":"input"}]"#,
        r#"[{"item":"fields::input"}]"#,
    ] {
        assert!(parse_cases(invalid).is_err(), "accepted {invalid}");
    }
}

#[test]
fn card_height_preserves_default_and_rounds_up_without_scaling() {
    assert_eq!(bounded_height(200.), HEIGHT);
    assert_eq!(bounded_height(600.), HEIGHT);
    assert_eq!(bounded_height(600.25), 601);
    assert_eq!(bounded_height(MAX_HEIGHT as f32), MAX_HEIGHT);
    assert!(std::panic::catch_unwind(|| bounded_height(MAX_HEIGHT as f32 + 1.)).is_err());
    assert!(std::panic::catch_unwind(|| bounded_height(f32::NAN)).is_err());
}

#[test]
fn minimum_height_is_bounded_and_never_scales_card_content() {
    assert_eq!(parse_minimum_height("600").unwrap(), HEIGHT);
    assert_eq!(parse_minimum_height("1088").unwrap(), 1088);
    assert_eq!(parse_minimum_height("4096").unwrap(), MAX_HEIGHT);
    for invalid in ["", "599", "4097", "1088.5", "-1", " 600", "4294967296"] {
        assert!(parse_minimum_height(invalid).is_err(), "accepted {invalid}");
    }
}
