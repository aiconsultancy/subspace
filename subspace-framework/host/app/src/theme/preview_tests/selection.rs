use gpui_kit::TestAppContext;
use gpui_kit::component::Theme;
use subspace_core::state::State;
use subspace_dashboard::density::{self, Density};
use subspace_dashboard::prefs;

use super::{assert_saved_fonts, init_app, installed_choices};
use crate::theme::{
    AppearanceSnapshot, FONT_PREF, FontChoices, HEADING_PREF, THEME_PREF, font_names, initialize,
    preview_selection, save_selection,
};

#[gpui_kit::test]
fn saved_selection_and_snapshot_cancel_keep_the_saved_appearance(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let state_file = root.path().join("state.json");
    let mut expected_fonts = FontChoices::default();
    cx.update(|cx| {
        init_app(cx, root.path(), state_file.clone());
        prefs::set(cx, THEME_PREF, "Solarized Dark");
        initialize(cx);
        expected_fonts = installed_choices(cx);
        cx.global_mut::<Density>().set("--panel-w", 731.0);
        save_selection("Solarized Dark", &expected_fonts, cx).unwrap();
        let saved = AppearanceSnapshot::capture(cx);
        preview_selection("Solarized Light", &FontChoices::default(), cx).unwrap();
        cx.global_mut::<Density>().set("--panel-w", 999.0);
        saved.restore(cx);
        assert_eq!(cx.global::<Density>().get("--panel-w"), 731.0);
        let resolved = crate::theme::fonts::resolve(
            &expected_fonts.heading,
            &expected_fonts.body,
            &expected_fonts.mono,
            &font_names(cx),
        );
        assert_eq!(
            Theme::global(cx).font_family.as_ref(),
            resolved.body.as_str()
        );
    });

    let state = State::load(&state_file);
    assert_eq!(state.get(THEME_PREF), Some("Solarized Dark"));
    assert_eq!(
        state.get(density::STATE_KEY),
        Some(r#"{"--panel-w":"731px"}"#)
    );
    assert_saved_fonts(&state, &expected_fonts);
}

#[gpui_kit::test]
fn failed_selection_save_rolls_back_preferences_and_later_flush(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let blocker = root.path().join("not-a-directory");
    std::fs::write(&blocker, "block parent creation").unwrap();
    let state_file = blocker.join("state.json");
    cx.update(|cx| {
        init_app(cx, root.path(), state_file.clone());
        prefs::set(cx, THEME_PREF, "Solarized Light");
        prefs::set(cx, FONT_PREF, "legacy");
        prefs::set(cx, density::STATE_KEY, r#"{"--row-h":"32px"}"#);
        initialize(cx);
        let attempted = installed_choices(cx);
        assert!(save_selection("Solarized Dark", &attempted, cx).is_err());
        assert_eq!(
            prefs::get(cx, THEME_PREF).as_deref(),
            Some("Solarized Light")
        );
        assert_eq!(prefs::get(cx, FONT_PREF).as_deref(), Some("legacy"));
        assert_eq!(prefs::get(cx, HEADING_PREF), None);
        assert_eq!(
            prefs::get(cx, density::STATE_KEY).as_deref(),
            Some(r#"{"--row-h":"32px"}"#)
        );
    });

    std::fs::remove_file(&blocker).unwrap();
    cx.update(|cx| prefs::save(cx).unwrap());
    let state = State::load(&state_file);
    assert_eq!(state.get(THEME_PREF), Some("Solarized Light"));
    assert_eq!(state.get(FONT_PREF), Some("legacy"));
    assert_eq!(state.get(HEADING_PREF), None);
    assert_ne!(state.get(THEME_PREF), Some("Solarized Dark"));
}
