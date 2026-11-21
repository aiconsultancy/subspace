//! Which docks the workspace shows. Both are preferences, saved with the
//! Space's other choices, and both show until the person hides them. The
//! rule repeats the Actions Space's `docks.rs`.

use gpui_kit::{App, Context};
use subspace_framework::dashboard::prefs;
use subspace_framework::dashboard::workspace::{SpaceDock, SpaceDocks};

use crate::Decisions;

const NAVIGATOR_KEY: &str = "decisions.navigator";
const INSPECTOR_KEY: &str = "decisions.inspector";
const SHOWN: &str = "shown";
const HIDDEN: &str = "hidden";

/// Whether the navigator and the inspector show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Docks {
    pub(crate) navigator: bool,
    pub(crate) inspector: bool,
}

impl Default for Docks {
    fn default() -> Self {
        Self {
            navigator: true,
            inspector: true,
        }
    }
}

impl Docks {
    /// The saved choice of each dock.
    pub(crate) fn load(cx: &App) -> Self {
        Self {
            navigator: shown(prefs::get(cx, NAVIGATOR_KEY).as_deref()),
            inspector: shown(prefs::get(cx, INSPECTOR_KEY).as_deref()),
        }
    }
}

/// A dock shows unless its saved value hides it, so a first run and an
/// unknown value both show it.
fn shown(saved: Option<&str>) -> bool {
    saved != Some(HIDDEN)
}

/// The value saved for a dock that shows, or is hidden.
fn saved_value(shown: bool) -> &'static str {
    if shown { SHOWN } else { HIDDEN }
}

impl Decisions {
    pub(crate) fn toggle_navigator(&mut self, cx: &mut Context<Self>) {
        self.docks.navigator = !self.docks.navigator;
        prefs::set(cx, NAVIGATOR_KEY, saved_value(self.docks.navigator));
        cx.notify();
    }

    /// Hiding the inspector keeps the selection and the open decision.
    pub(crate) fn toggle_inspector(&mut self, cx: &mut Context<Self>) {
        self.docks.inspector = !self.docks.inspector;
        prefs::set(cx, INSPECTOR_KEY, saved_value(self.docks.inspector));
        cx.notify();
    }

    /// Both docks, as they show, for the status bar's switches.
    pub(crate) fn space_docks(&self) -> SpaceDocks {
        SpaceDocks::both(self.docks.navigator, self.docks.inspector)
    }

    /// A dock's status bar switch was used, the same toggle as the palette's.
    pub(crate) fn toggle_space_dock(&mut self, dock: SpaceDock, cx: &mut Context<Self>) {
        match dock {
            SpaceDock::Navigator => self.toggle_navigator(cx),
            SpaceDock::Inspector => self.toggle_inspector(cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::TestAppContext;
    use subspace_framework::dashboard::prefs;

    use super::{Docks, saved_value, shown};

    #[gpui_kit::test]
    fn a_dock_hidden_in_the_preferences_loads_hidden(cx: &mut TestAppContext) {
        cx.update(|cx| {
            prefs::init_file(None, cx);
            assert_eq!(Docks::load(cx), Docks::default());
            prefs::set(cx, "decisions.navigator", "hidden");
            let docks = Docks::load(cx);
            assert!(!docks.navigator && docks.inspector);
        });
    }

    #[test]
    fn both_docks_show_until_they_are_hidden() {
        let docks = Docks::default();
        assert!(docks.navigator && docks.inspector);
        assert!(shown(None), "nothing saved yet");
        assert!(shown(Some("anything else")), "an unknown value shows");
        for choice in [true, false] {
            assert_eq!(shown(Some(saved_value(choice))), choice);
        }
    }
}
