//! Whether the navigator starts with the Space's title. A saved choice
//! (Settings › Layout) wins; without one, the app's default applies, and
//! without an app default the title shows.
//!
//! The host app installs the default and Settings saves the choice. The
//! setters and `preview` are public only because those callers live in
//! other crates: they are reserved for the host app and the Component
//! gallery, and hidden from the docs. Nothing stops a Space calling them,
//! so a Space must leave the choice to the person.

use gpui_kit::{App, Global};

use crate::prefs;

/// The saved choice: `"1"` shows the title, `"0"` hides it.
const KEY: &str = "layout.navigator-title";

/// The app's default, installed once at start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NavigatorTitleDefault(bool);

impl Global for NavigatorTitleDefault {}

/// Installs the app's default for people who have not chosen. Reserved for
/// the host app's start-up, which calls it from `ProductBuild::navigator_title`.
#[doc(hidden)]
pub fn install_default(shown: bool, cx: &mut App) {
    cx.set_global(NavigatorTitleDefault(shown));
}

/// Whether navigators show the Space's title.
pub fn shown(cx: &App) -> bool {
    let default = cx
        .try_global::<NavigatorTitleDefault>()
        .is_none_or(|default| default.0);
    decide(prefs::get(cx, KEY).as_deref(), default)
}

/// Records the person's choice in the preferences. Reserved for Settings
/// Save, which calls it inside its state file write.
#[doc(hidden)]
pub fn set_shown(shown: bool, cx: &mut App) {
    prefs::set(cx, KEY, if shown { "1" } else { "0" });
}

/// Draws `navigator` as the title setting would, whatever the person chose.
/// Reserved for the Component gallery's samples; a Space should not call
/// it, as the choice is the person's.
#[doc(hidden)]
pub fn preview(mut navigator: super::Navigator, shown: bool) -> super::Navigator {
    navigator.title_preview = Some(shown);
    navigator
}

/// A saved `"1"` or `"0"` wins over `default`; anything else is no choice.
fn decide(saved: Option<&str>, default: bool) -> bool {
    match saved {
        Some("1") => true,
        Some("0") => false,
        _ => default,
    }
}

#[cfg(test)]
mod tests {
    use super::decide;

    #[test]
    fn a_saved_choice_wins_over_the_default() {
        assert!(decide(None, true));
        assert!(!decide(None, false));
        assert!(decide(Some("1"), false));
        assert!(!decide(Some("0"), true));
        assert!(!decide(Some("other"), false));
    }
}
