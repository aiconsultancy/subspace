use gpui_kit::TestAppContext;
use subspace_core::state::State;
use subspace_dashboard::density::{self, Density};
use subspace_dashboard::prefs;

use super::{assert_saved_fonts, init_app};
use crate::theme::{self, FONT_PREF, FontChoices, initialize, preview_selection, save_selection};

#[gpui_kit::test]
fn font_only_preview_keeps_the_live_density(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        init_app(cx, root.path(), root.path().join("state.json"));
        initialize(cx);
        cx.global_mut::<Density>().set("--row-h", 41.0);
        preview_selection(&theme::selected_theme(cx), &FontChoices::default(), cx).unwrap();
        assert_eq!(cx.global::<Density>().get("--row-h"), 41.0);
        assert!(prefs::get(cx, density::STATE_KEY).is_none());
    });
}

#[gpui_kit::test]
fn missing_saved_fonts_remain_selectable_and_report_each_fallback(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        init_app(cx, root.path(), root.path().join("state.json"));
        prefs::set(cx, FONT_PREF, "Unavailable family");
        initialize(cx);
        let choices = theme::selected_fonts(cx);
        assert!(theme::font_choices(cx).contains(&choices.body));
        let warnings = preview_selection(&theme::selected_theme(cx), &choices, cx).unwrap();
        assert_eq!(warnings.len(), 3);
        assert!(warnings.iter().any(|warning| warning.contains("Body text")));
        save_selection(&theme::selected_theme(cx), &choices, cx).unwrap();
        let state = State::load(&root.path().join("state.json"));
        assert_saved_fonts(&state, &choices);
        assert!(state.get(FONT_PREF).is_none());
    });
}
