use std::path::{Path, PathBuf};
use std::sync::Arc;

mod defaults;
mod font_overrides;
mod selection;

use gpui_kit::component::{Theme, try_parse_color};
use gpui_kit::{App, BorrowAppContext as _, TestAppContext};
use serde_json::json;
use subspace_core::state::State;
use subspace_dashboard::density::{self, Density, DensityPreview};
use subspace_dashboard::tokens::ThemeTones;
use subspace_dashboard::typography::Typography;
use subspace_dashboard::{CurrentProduct, prefs};
use subspace_product::{OsKind, PlatformEnv, ProductContext, fixture};

use super::{
    AppearanceSnapshot, BODY_PREF, FONT_PREF, FontChoices, GroupHeadingChoices, HEADING_PREF,
    MONO_PREF, THEME_PREF, apply_document, document, font_names, initialize, preview_selection,
};
use crate::product::ProductFiles;

fn product_context(root: &Path) -> ProductContext {
    let known = fixture::products().unwrap();
    let product = known[0].clone();
    let env = PlatformEnv {
        os: OsKind::Linux,
        home: root.join("home"),
        xdg_config_home: Some(root.join("config")),
        xdg_state_home: Some(root.join("state")),
        app_data: None,
        local_app_data: None,
    };
    ProductContext::new(product, env)
}

/// Clear the active theme's control tokens, so a layout test measures the
/// nominal global icon-button scale rather than one theme's button sizes.
pub(crate) fn nominal_controls(cx: &mut App) {
    cx.update_default_global::<Density, _>(|density, _| {
        let groups = &density::GROUPS[density::CONTROL_GROUPS];
        for control in groups.iter().flat_map(|group| group.controls) {
            density.remove(control.token);
        }
    });
}

pub(crate) fn init_app(cx: &mut App, root: &Path, state_file: PathBuf) {
    gpui_kit::init(cx);
    let context = Arc::new(product_context(root));
    cx.set_global(CurrentProduct(context.clone()));
    cx.set_global(ProductFiles {
        config: root.join("config.json"),
        state: state_file.clone(),
    });
    prefs::init_file(Some(state_file), cx);
}

fn write_state(file: &Path, values: &[(&str, &str)]) {
    let mut state = State::default();
    for (key, value) in values {
        state.set(*key, *value);
    }
    state.save(file).unwrap();
}

fn installed_choices(cx: &App) -> FontChoices {
    let names = font_names(cx);
    let at = |index: usize| {
        names
            .get(index)
            .or_else(|| names.first())
            .cloned()
            .unwrap_or_default()
    };
    FontChoices {
        heading: at(0),
        body: at(1),
        mono: at(2),
    }
}

fn assert_saved_fonts(state: &State, expected: &FontChoices) {
    assert_eq!(state.get(HEADING_PREF), Some(expected.heading.as_str()));
    assert_eq!(state.get(BODY_PREF), Some(expected.body.as_str()));
    assert_eq!(state.get(MONO_PREF), Some(expected.mono.as_str()));
    assert_eq!(state.get(FONT_PREF), None);
}

#[gpui_kit::test]
fn initialize_uses_theme_role_defaults_without_rewriting_preferences(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let state_file = root.path().join("state.json");
    let density = r#"{"--row-h":"33px"}"#;
    write_state(
        &state_file,
        &[
            (THEME_PREF, "Solarized Light"),
            (FONT_PREF, ""),
            (density::STATE_KEY, density),
        ],
    );

    cx.update(|cx| {
        init_app(cx, root.path(), state_file.clone());
        let installed = font_names(cx);
        let expected = super::fonts::resolve("", "", "", &installed);
        initialize(cx);
        assert_eq!(
            cx.global::<Typography>().heading.as_ref(),
            expected.heading.as_str()
        );
        assert_eq!(
            Theme::global(cx).font_family.as_ref(),
            expected.body.as_str()
        );
        assert_eq!(
            Theme::global(cx).mono_font_family.as_ref(),
            expected.mono.as_str()
        );
        assert_eq!(cx.global::<Density>().get("--row-h"), 33.0);
    });

    let state = State::load(&state_file);
    assert_eq!(state.get(THEME_PREF), Some("Solarized Light"));
    assert_eq!(state.get(FONT_PREF), Some(""));
    assert_eq!(state.get(density::STATE_KEY), Some(density));
}

#[gpui_kit::test]
fn initialize_keeps_legacy_explicit_font_and_density_preferences(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let state_file = root.path().join("state.json");
    cx.update(|cx| {
        init_app(cx, root.path(), state_file.clone());
        let font = "Legacy font";
        prefs::set(cx, THEME_PREF, "Solarized Dark");
        prefs::set(cx, FONT_PREF, font);
        prefs::set(cx, density::STATE_KEY, r#"{"--panel-w":"777px"}"#);
        prefs::save(cx).unwrap();
        let before = std::fs::read(&state_file).unwrap();
        let warnings = initialize(cx);
        assert_eq!(std::fs::read(&state_file).unwrap(), before);
        let resolved = super::fonts::resolve(font, font, font, &font_names(cx));
        assert_eq!(Theme::global(cx).font_family.as_ref(), resolved.body);
        assert_eq!(Theme::global(cx).mono_font_family.as_ref(), resolved.mono);
        assert_eq!(super::selected_fonts(cx).body, font);
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("Legacy font"))
        );
        assert_eq!(cx.global::<Density>().get("--panel-w"), 777.0);
    });
}

#[gpui_kit::test]
fn draft_document_survives_periodic_save_only_in_live_globals(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let state_file = root.path().join("state.json");
    write_state(
        &state_file,
        &[(THEME_PREF, "Solarized Light"), (density::STATE_KEY, "{}")],
    );
    let before = std::fs::read(&state_file).unwrap();

    cx.update(|cx| {
        init_app(cx, root.path(), state_file.clone());
        initialize(cx);
        let fonts = installed_choices(cx);
        let resolved =
            super::fonts::resolve(&fonts.heading, &fonts.body, &fonts.mono, &font_names(cx));
        let mut draft = document("Solarized Light", cx).unwrap();
        draft.set_color("background", "#123456").unwrap();
        let mut metadata = draft.metadata().unwrap();
        metadata.fonts = fonts.clone();
        metadata.group_heading = GroupHeadingChoices {
            font: fonts.body.clone(),
            size: 14.5,
            bold: false,
            italic: true,
        };
        metadata.density = json!({"--row-h": "41px"});
        draft.set_metadata(metadata).unwrap();
        apply_document(&draft, &FontChoices::default(), cx).unwrap();
        prefs::save(cx).unwrap();
        assert_eq!(
            Theme::global(cx).colors.background,
            try_parse_color("#123456").unwrap()
        );
        assert_eq!(
            cx.global::<Typography>().heading.as_ref(),
            resolved.heading.as_str()
        );
        assert_eq!(
            Theme::global(cx).font_family.as_ref(),
            resolved.body.as_str()
        );
        assert_eq!(
            Theme::global(cx).mono_font_family.as_ref(),
            resolved.mono.as_str()
        );
        assert_eq!(cx.global::<Density>().get("--row-h"), 41.0);
        let group = &cx.global::<Typography>().group_heading;
        assert_eq!(group.font.as_ref(), resolved.heading);
        assert_eq!(group.size, 14.5);
        assert!(!group.bold);
        assert!(group.italic);
    });

    assert_eq!(std::fs::read(&state_file).unwrap(), before);
}

#[gpui_kit::test]
fn appearance_snapshot_restores_every_preview_global_exactly(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        init_app(cx, root.path(), root.path().join("state.json"));
        initialize(cx);
        cx.global_mut::<Density>().set("--row-h", 35.0);
        cx.set_global(DensityPreview(true));
        let snapshot = AppearanceSnapshot::capture(cx);
        let colors = Theme::global(cx).colors;
        let body = Theme::global(cx).font_family.clone();
        let mono = Theme::global(cx).mono_font_family.clone();
        let heading = cx.global::<Typography>().heading.clone();
        let group_heading = cx.global::<Typography>().group_heading.clone();
        let tones = cx.global::<ThemeTones>().0.clone();
        let density = cx.global::<Density>().clone();

        preview_selection("Solarized Dark", &installed_choices(cx), cx).unwrap();
        cx.global_mut::<Density>().set("--row-h", 47.0);
        cx.set_global(DensityPreview(false));
        snapshot.restore(cx);
        assert_eq!(Theme::global(cx).colors, colors);
        assert_eq!(Theme::global(cx).font_family, body);
        assert_eq!(Theme::global(cx).mono_font_family, mono);
        assert_eq!(cx.global::<Typography>().heading, heading);
        assert_eq!(cx.global::<Typography>().group_heading, group_heading);
        assert_eq!(cx.global::<ThemeTones>().0, tones);
        assert_eq!(cx.global::<Density>(), &density);
        assert!(cx.global::<DensityPreview>().0);
    });
}

#[gpui_kit::test]
fn density_preview_blocks_panel_persistence(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        init_app(cx, root.path(), root.path().join("state.json"));
        prefs::set(cx, density::STATE_KEY, r#"{"--row-h":"31px"}"#);
        cx.set_global(Density::from_json(r#"{"--row-h":"44px"}"#));
        cx.set_global(DensityPreview(true));
        subspace_dashboard::panel::save_density(cx);
        assert_eq!(
            prefs::get(cx, density::STATE_KEY).as_deref(),
            Some(r#"{"--row-h":"31px"}"#)
        );
    });
}
