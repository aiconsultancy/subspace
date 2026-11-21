//! The docks a Space tells the shell about, and the switch that shows or
//! hides each. The shell's status bar draws the navigator's switch at its
//! leading end and the inspector's at its trailing end, for the docks the
//! active Space reports through `Dashboard::docks`, and hands a click back
//! through `Dashboard::toggle_dock`.

use gpui_kit::{App, ClickEvent, Window};
use subspace_lucide::IconName;

use crate::action::{Intent, icon_button};
use crate::icon_button::IconButton;

/// One of a Space's two docks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceDock {
    /// The leading dock: where you are looking.
    Navigator,
    /// The trailing dock: the detail of the selection.
    Inspector,
}

impl SpaceDock {
    pub const ALL: [Self; 2] = [Self::Navigator, Self::Inspector];

    /// The switch's element id, the same in every Space.
    pub fn switch_id(self) -> &'static str {
        match self {
            Self::Navigator => "workspace-navigator-toggle",
            Self::Inspector => "workspace-inspector-toggle",
        }
    }

    /// The switch's accessible name and tooltip: what pressing it does.
    pub fn switch_name(self, shown: bool) -> &'static str {
        match (self, shown) {
            (Self::Navigator, true) => "Hide navigator",
            (Self::Navigator, false) => "Show navigator",
            (Self::Inspector, true) => "Hide inspector",
            (Self::Inspector, false) => "Show inspector",
        }
    }

    fn glyph(self) -> IconName {
        match self {
            Self::Navigator => IconName::PanelLeft,
            Self::Inspector => IconName::PanelRight,
        }
    }
}

/// The docks a Space has, and whether each shows now. `None` means the Space
/// has no such dock where it is, so the shell draws no switch for it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpaceDocks {
    pub navigator: Option<bool>,
    pub inspector: Option<bool>,
}

impl SpaceDocks {
    /// No docks: a Space that is not laid out on the workspace shell, such
    /// as a web Space.
    pub const NONE: Self = Self {
        navigator: None,
        inspector: None,
    };

    /// Both docks, each shown or hidden.
    // qual:api — the desktop's Spaces (Actions, Channels, Decisions, GitHub,
    // GitHub Copilot agents) report their docks with it; no Framework Space
    // has both docks.
    pub const fn both(navigator: bool, inspector: bool) -> Self {
        Self {
            navigator: Some(navigator),
            inspector: Some(inspector),
        }
    }

    /// Whether `dock` shows, or `None` when the Space has no such dock.
    pub fn get(self, dock: SpaceDock) -> Option<bool> {
        match dock {
            SpaceDock::Navigator => self.navigator,
            SpaceDock::Inspector => self.inspector,
        }
    }
}

/// A dock's switch: a quiet panel glyph that takes the accent colour while
/// the dock shows, with no fill. Its id and name are the same in every Space.
pub fn dock_switch(
    dock: SpaceDock,
    shown: bool,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> IconButton {
    icon_button(
        dock.switch_id(),
        Intent::Quiet,
        dock.glyph(),
        dock.switch_name(shown),
    )
    .latched_glyph(shown)
    .on_click(on_toggle)
}
