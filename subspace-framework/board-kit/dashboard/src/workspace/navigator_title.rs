//! Whether the navigator starts with the Space's title. A saved choice
//! (Settings › Layout) wins; without one, the app's default applies, and
//! without an app default the title shows.

use gpui_kit::{App, Global};

use crate::prefs;

/// The saved choice: `"1"` shows the title, `"0"` hides it.
pub const KEY: &str = "layout.navigator-title";

/// The app's default, installed once at start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavigatorTitleDefault(pub bool);

impl Global for NavigatorTitleDefault {}

/// Installs the app's default for people who have not chosen.
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

/// Saves the choice; Settings Save calls it.
pub fn set_shown(shown: bool, cx: &mut App) {
    prefs::set(cx, KEY, if shown { "1" } else { "0" });
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
