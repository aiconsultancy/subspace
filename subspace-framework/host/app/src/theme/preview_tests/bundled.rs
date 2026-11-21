//! Fonts the app registers at startup count as installed for a theme that
//! names them, and are not automatic choices for themes that name none.

use gpui_kit::TestAppContext;
use gpui_kit::component::Theme;
use subspace_dashboard::prefs;

use super::init_app;
use crate::BundledFont;
use crate::theme::{THEME_PREF, font_names, initialize, start};

/// Fictional bytes: the test text system loads no font data, so only the
/// family names matter.
const FONTS: &[BundledFont] = &[
    BundledFont::new("Inter", b"fixture sans"),
    BundledFont::new("JetBrains Mono", b"fixture mono"),
];

#[gpui_kit::test]
fn a_theme_naming_registered_families_starts_without_font_notices(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        init_app(cx, root.path(), root.path().join("state.json"));
        prefs::set(cx, THEME_PREF, "Twilight Two");
        let unregistered = initialize(cx);
        assert!(
            unregistered
                .iter()
                .any(|warning| warning.contains("Inter is not installed")),
            "{unregistered:?}"
        );

        let notices = start(FONTS, cx);

        assert!(notices.is_empty(), "{notices:?}");
        assert_eq!(Theme::global(cx).font_family.as_ref(), "Inter");
        assert_eq!(
            Theme::global(cx).mono_font_family.as_ref(),
            "JetBrains Mono"
        );
    });
}

#[gpui_kit::test]
fn registered_families_are_not_automatic_choices_for_other_themes(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        init_app(cx, root.path(), root.path().join("state.json"));
        prefs::set(cx, THEME_PREF, "Solarized Light");

        start(FONTS, cx);

        assert_ne!(Theme::global(cx).font_family.as_ref(), "Inter");
        assert_ne!(
            Theme::global(cx).mono_font_family.as_ref(),
            "JetBrains Mono"
        );
        let installed = font_names(cx);
        for family in ["Inter", "JetBrains Mono"] {
            assert!(installed.iter().any(|name| name == family), "{family}");
        }
    });
}
