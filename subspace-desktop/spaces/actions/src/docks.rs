//! Which docks the workspace shows. Both are preferences, saved with the
//! view and the filters, and both show until the user hides them.

use gpui_kit::{App, Context};
use subspace_framework::dashboard::prefs;
use subspace_framework::dashboard::workspace::{SpaceDock, SpaceDocks};

use crate::Actions;

const NAVIGATOR_KEY: &str = "actions.navigator";
const INSPECTOR_KEY: &str = "actions.inspector";
const SHOWN: &str = "shown";
const HIDDEN: &str = "hidden";

/// Whether the navigator and the run's detail panel show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Docks {
    pub navigator: bool,
    /// The right-hand detail panel of the open run, and Flow's job panel.
    pub inspector: bool,
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
pub(crate) fn shown(saved: Option<&str>) -> bool {
    saved != Some(HIDDEN)
}

/// The value saved for a dock that shows, or is hidden.
pub(crate) fn saved_value(shown: bool) -> &'static str {
    if shown { SHOWN } else { HIDDEN }
}

impl Actions {
    pub(crate) fn toggle_navigator(&mut self, cx: &mut Context<Self>) {
        self.docks.navigator = !self.docks.navigator;
        prefs::set(cx, NAVIGATOR_KEY, saved_value(self.docks.navigator));
        cx.notify();
    }

    /// Hiding the inspector keeps the open run open: its jobs still load for
    /// Flow, and the panel comes back as it was.
    pub(crate) fn toggle_inspector(&mut self, cx: &mut Context<Self>) {
        self.docks.inspector = !self.docks.inspector;
        prefs::set(cx, INSPECTOR_KEY, saved_value(self.docks.inspector));
        cx.notify();
    }

    /// Both docks, as they show, for the status bar's switches.
    pub(crate) fn space_docks(&self) -> SpaceDocks {
        SpaceDocks::both(self.docks.navigator, self.docks.inspector)
    }

    /// A dock's status bar switch was used.
    pub(crate) fn toggle_space_dock(&mut self, dock: SpaceDock, cx: &mut Context<Self>) {
        match dock {
            SpaceDock::Navigator => self.toggle_navigator(cx),
            SpaceDock::Inspector => self.toggle_inspector(cx),
        }
    }

    /// Shows the inspector for something that only it can show, such as a
    /// job log the user asked for.
    pub(crate) fn show_inspector(&mut self, cx: &mut Context<Self>) {
        if !self.docks.inspector {
            self.toggle_inspector(cx);
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
            prefs::set(cx, "actions.inspector", "hidden");
            let docks = Docks::load(cx);
            assert!(docks.navigator && !docks.inspector);
        });
    }

    #[test]
    fn both_docks_show_until_they_are_hidden() {
        let docks = Docks::default();
        assert!(docks.navigator && docks.inspector);
        assert!(shown(None), "nothing saved yet");
        assert!(shown(Some("shown")));
        assert!(shown(Some("anything else")), "an unknown value shows");
        assert!(!shown(Some("hidden")));
    }

    #[test]
    fn a_saved_choice_reads_back_as_it_was_set() {
        for choice in [true, false] {
            assert_eq!(shown(Some(saved_value(choice))), choice);
        }
    }
}
