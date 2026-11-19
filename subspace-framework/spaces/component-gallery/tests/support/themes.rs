use std::path::Path;
use std::rc::Rc;

use gpui_kit::component::{ActiveTheme as _, Theme, ThemeConfig, ThemeSet};
use gpui_kit::{App, HeadlessAppContext, px};
use serde_json::Value;
use subspace_dashboard::Tokens;
use subspace_dashboard::density::{BODY_FONT_DEFAULT, Density, REFERENCE_REM};
use subspace_dashboard::tokens::ThemeTones;

use super::settings::{BODY, MONO};

pub(crate) fn require_fonts(cx: &HeadlessAppContext) {
    let installed = cx.text_system().all_font_names();
    for family in [BODY, MONO] {
        assert!(
            installed.iter().any(|name| name == family),
            "required font family {family:?} is absent; refusing font fallback"
        );
    }
}

pub(crate) struct ThemeFixture {
    config: Rc<ThemeConfig>,
    density: Density,
    tones: ThemeTones,
}

pub(crate) fn load_theme(path: &Path, name: &str) -> ThemeFixture {
    let text = std::fs::read_to_string(path).expect("read explicit theme fixture JSON");
    let set: ThemeSet = serde_json::from_str(&text).expect("parse with public GPUI ThemeSet");
    let config = set
        .themes
        .into_iter()
        .find(|config| config.name.as_ref() == name)
        .unwrap_or_else(|| panic!("theme fixture must contain {name:?}"));
    let document: Value = serde_json::from_str(&text).expect("parse theme extension metadata");
    let variant = document["themes"]
        .as_array()
        .expect("theme variants")
        .iter()
        .find(|variant| variant["name"] == name)
        .expect("matching metadata variant");
    let metadata = &variant["subspace"];
    assert_eq!(metadata["version"], 1, "supported Subspace theme metadata");
    let tones = metadata["tones"]
        .as_object()
        .expect("theme tone metadata")
        .iter()
        .map(|(key, value)| {
            let color = gpui_kit::component::try_parse_color(value.as_str().expect("tone string"))
                .expect("valid theme tone");
            (key.clone(), color)
        })
        .collect();
    ThemeFixture {
        config: Rc::new(config),
        density: Density::from_json(&metadata["density"].to_string()),
        tones: ThemeTones(tones),
    }
}

pub(crate) fn apply_theme(fixture: &ThemeFixture, cx: &mut App) {
    cx.set_global(fixture.density.clone());
    cx.set_global(fixture.tones.clone());
    // Public theme configuration and dashboard globals, never a host dependency.
    Theme::update(cx, |theme| {
        theme.apply_config(&fixture.config);
        subspace_dashboard::toggle::track::apply(theme);
        theme.font_family = BODY.into();
        theme.mono_font_family = MONO.into();
    });
    let tokens = Tokens::current(cx);
    Theme::update(cx, |theme| {
        theme.font_size = px(REFERENCE_REM) * (f32::from(tokens.row_font) / BODY_FONT_DEFAULT);
        theme.mono_font_size = tokens.mono_font;
    });
    assert_eq!(cx.theme().theme_name(), &fixture.config.name);
    assert!(cx.theme().is_dark());
    assert_eq!(cx.theme().font_family.as_ref(), BODY);
    assert_eq!(cx.theme().mono_font_family.as_ref(), MONO);
}
