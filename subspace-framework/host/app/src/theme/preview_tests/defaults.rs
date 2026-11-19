use gpui_kit::TestAppContext;
use subspace_dashboard::density::{self, Density};
use subspace_dashboard::prefs;

use super::init_app;
use crate::theme::{THEME_PREF, initialize, selected_theme};

#[gpui_kit::test]
fn startup_applies_the_product_default_but_keeps_explicit_user_choices(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        init_app(cx, root.path(), root.path().join("state.json"));
        initialize(cx);
        assert_eq!(selected_theme(cx), "Cupertino Dark");
        assert_eq!(prefs::get(cx, THEME_PREF), None);

        prefs::set(cx, THEME_PREF, "Solarized Light");
        prefs::set(cx, density::STATE_KEY, r#"{"--panel-w":"731px"}"#);
        initialize(cx);
        assert_eq!(selected_theme(cx), "Solarized Light");
        assert_eq!(cx.global::<Density>().get("--panel-w"), 731.0);
        assert_eq!(
            prefs::get(cx, THEME_PREF).as_deref(),
            Some("Solarized Light")
        );
    });
}
