//! Which docks the Space shows: the navigator and the inspector. The person's
//! two choices are saved with the Space's other preferences, and both show
//! until they are hidden. A window too narrow for the docks and the pane holds
//! a dock back without changing the choice: the inspector first, then the
//! navigator.

use gpui_kit::{App, Context};
use subspace_framework::dashboard::workspace::{SpaceDock, SpaceDocks};
use subspace_framework::dashboard::{DashboardEvent, Toast, Tokens, prefs, workspace};

use crate::Issues;

const NAVIGATOR_PREF: &str = "issues.navigator";
const INSPECTOR_PREF: &str = "issues.inspector";
const SHOWN: &str = "shown";
const HIDDEN: &str = "hidden";
/// The narrowest the pane gets before a dock gives way to it.
const PANE_MIN_WIDTH: f32 = 440.;
/// A window width before the first render, when both choices stand.
const UNKNOWN_WIDTH: f32 = 0.;
/// What a hidden dock takes from the pane.
const NO_WIDTH: f32 = 0.;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dock {
    Navigator,
    Inspector,
}

impl From<SpaceDock> for Dock {
    fn from(dock: SpaceDock) -> Self {
        match dock {
            SpaceDock::Navigator => Dock::Navigator,
            SpaceDock::Inspector => Dock::Inspector,
        }
    }
}

impl Dock {
    fn pref(self) -> &'static str {
        match self {
            Dock::Navigator => NAVIGATOR_PREF,
            Dock::Inspector => INSPECTOR_PREF,
        }
    }

    /// The words a toast gives when the window holds the dock back.
    fn held_back(self) -> &'static str {
        match self {
            Dock::Navigator => "The window is too narrow to show the navigator.",
            Dock::Inspector => "The window is too narrow to show the inspector.",
        }
    }
}

/// Whether the navigator and the inspector show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub(crate) fn shows(self, dock: Dock) -> bool {
        match dock {
            Dock::Navigator => self.navigator,
            Dock::Inspector => self.inspector,
        }
    }

    fn with(mut self, dock: Dock, shown: bool) -> Self {
        match dock {
            Dock::Navigator => self.navigator = shown,
            Dock::Inspector => self.inspector = shown,
        }
        self
    }
}

/// The widths the docks and the pane share: what the workspace has, and each
/// dock as it is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Room {
    pub(crate) width: f32,
    pub(crate) navigator: f32,
    pub(crate) inspector: f32,
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

/// The saved choice of each dock.
pub(crate) fn saved(cx: &App) -> Docks {
    Docks {
        navigator: shown(prefs::get(cx, NAVIGATOR_PREF).as_deref()),
        inspector: shown(prefs::get(cx, INSPECTOR_PREF).as_deref()),
    }
}

/// The docks that show: the chosen ones, while the pane keeps its least
/// width. The inspector gives way first, then the navigator. Before the
/// first render the width is unknown and the choices stand.
pub(crate) fn fit(chosen: Docks, room: Room) -> Docks {
    if room.width <= UNKNOWN_WIDTH {
        return chosen;
    }
    let navigator = if chosen.navigator {
        room.navigator
    } else {
        NO_WIDTH
    };
    let beside_navigator = room.width - navigator;
    let inspector = chosen.inspector && beside_navigator - room.inspector >= PANE_MIN_WIDTH;
    Docks {
        navigator: chosen.navigator && (inspector || beside_navigator >= PANE_MIN_WIDTH),
        inspector,
    }
}

/// The pane's width beside the docks that show.
pub(crate) fn pane_width(shown: Docks, room: Room) -> f32 {
    let navigator = if shown.navigator {
        room.navigator
    } else {
        NO_WIDTH
    };
    let inspector = if shown.inspector {
        room.inspector
    } else {
        NO_WIDTH
    };
    (room.width - navigator - inspector).max(NO_WIDTH)
}

/// The choices after a dock's switch is used: the dock that shows is hidden
/// and the hidden one is chosen. `None` when the dock is chosen already and
/// only the window's width holds it back.
pub(crate) fn toggled(chosen: Docks, shown: Docks, dock: Dock) -> Option<Docks> {
    if chosen.shows(dock) && !shown.shows(dock) {
        return None;
    }
    Some(chosen.with(dock, !shown.shows(dock)))
}

/// The choices for what shows now: the inspector only where there is one.
fn offered(chosen: Docks, inspector: bool) -> Docks {
    Docks {
        inspector: chosen.inspector && inspector,
        ..chosen
    }
}

// qual:allow(coupling, oi) reason: "The dock choices read the board's last window width, what it shows and its open side panel; the fitting rule itself is pure and tested apart."
impl Issues {
    /// The room at the last render. An open attachment or decision panel
    /// takes its width first.
    fn dock_room(&self, tokens: &Tokens) -> Room {
        Room {
            width: self.width - crate::view::issue::panel::side_width(self, tokens),
            navigator: workspace::navigator_width(f32::from(tokens.navigator_width), self.width),
            inspector: workspace::inspector_width(f32::from(tokens.inspector_width), self.width),
        }
    }

    /// Whether what shows has an inspector: the list's peek of the selected
    /// issue, or the open issue's Properties once it has loaded. The
    /// activity feed and the screens that stand in for the list have none.
    pub(crate) fn has_inspector(&self) -> bool {
        match &self.issue_view.snapshot {
            Some(open) => open.detail().is_some(),
            None => {
                !self.activity.visible
                    && !(crate::initiative::no_initiative_projects(self) && self.is_ready())
                    && crate::scope::body(&self.access, &self.listing).controls()
            }
        }
    }

    /// The docks that show now.
    pub(crate) fn shown_docks(&self, tokens: &Tokens) -> Docks {
        fit(
            offered(self.docks, self.has_inspector()),
            self.dock_room(tokens),
        )
    }

    /// The docks the status bar's switches show: the navigator always, the
    /// inspector only where there is one, each as it shows now.
    pub(crate) fn space_docks(&self, cx: &App) -> SpaceDocks {
        let shown = self.shown_docks(&Tokens::current(cx));
        SpaceDocks {
            navigator: Some(shown.navigator),
            inspector: self.has_inspector().then_some(shown.inspector),
        }
    }

    /// The pane's width beside the docks that show, less an open attachment
    /// or decision panel. It decides the list's lanes and the page's layout.
    pub(crate) fn pane_width(&self, tokens: &Tokens) -> f32 {
        pane_width(self.shown_docks(tokens), self.dock_room(tokens))
    }

    /// A dock's switch, or the inspector's Close.
    pub(crate) fn toggle_dock(&mut self, dock: Dock, cx: &mut Context<Self>) {
        let shown = self.shown_docks(&Tokens::current(cx));
        let Some(docks) = toggled(self.docks, shown, dock) else {
            cx.emit(DashboardEvent::Toast(Toast::info(dock.held_back())));
            return;
        };
        self.docks = docks;
        prefs::set(cx, dock.pref(), saved_value(docks.shows(dock)));
        cx.notify();
    }
}

#[cfg(test)]
#[path = "tests/docks.rs"]
mod tests;
