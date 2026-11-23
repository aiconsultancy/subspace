//! The app's **Icons** setting: how large fleet agents' and ships' icons
//! show in Spaces, or whether they show at all. Stored with the other
//! appearance choices and read by any Space through [`AgentIcons::current`].

use gpui_kit::{App, Global};

/// The stored choice's key in the app's state file.
pub const PREFERENCE_KEY: &str = "app.agent-icons";

/// The choice before anyone makes one, and for a stored value this version
/// does not know.
pub const DEFAULT_AGENT_ICONS: AgentIcons = AgentIcons::Small;

/// How agents' and ships' icons show: as their surface draws them today,
/// small (about the size of a row's leading glyph), or not at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AgentIcons {
    Large,
    Small,
    Off,
}

impl Global for AgentIcons {}

impl Default for AgentIcons {
    fn default() -> Self {
        DEFAULT_AGENT_ICONS
    }
}

impl AgentIcons {
    /// Every choice, in the order Settings lists them.
    pub const ALL: [Self; 3] = [Self::Large, Self::Small, Self::Off];

    /// The live choice; the default before one is applied.
    pub fn current(cx: &App) -> Self {
        cx.try_global::<Self>().copied().unwrap_or_default()
    }

    /// Makes this the live choice and redraws every window, so it applies
    /// without a restart.
    pub fn apply(self, cx: &mut App) {
        cx.set_global(self);
        cx.refresh_windows();
    }

    /// The stored value.
    pub fn key(self) -> &'static str {
        match self {
            Self::Large => "large",
            Self::Small => "small",
            Self::Off => "off",
        }
    }

    /// The choice's name in Settings.
    pub fn label(self) -> &'static str {
        match self {
            Self::Large => "Large",
            Self::Small => "Small",
            Self::Off => "Off",
        }
    }

    /// The choice stored as `value`; the default for none or one this
    /// version does not know.
    pub fn from_key(value: Option<&str>) -> Self {
        Self::ALL
            .into_iter()
            .find(|icons| Some(icons.key()) == value)
            .unwrap_or_default()
    }
}

/// Restores the stored choice once the product's preferences are loaded.
pub(crate) fn initialize(cx: &mut App) {
    AgentIcons::from_key(crate::prefs::get(cx, PREFERENCE_KEY).as_deref()).apply(cx);
}

#[cfg(test)]
mod tests {
    use super::{AgentIcons, DEFAULT_AGENT_ICONS};

    #[test]
    fn a_missing_or_unknown_value_reads_as_the_default_and_each_key_round_trips() {
        assert_eq!(DEFAULT_AGENT_ICONS, AgentIcons::Small);
        assert_eq!(AgentIcons::from_key(None), DEFAULT_AGENT_ICONS);
        assert_eq!(AgentIcons::from_key(Some("huge")), DEFAULT_AGENT_ICONS);
        for icons in AgentIcons::ALL {
            assert_eq!(AgentIcons::from_key(Some(icons.key())), icons);
        }
        let labels: Vec<_> = AgentIcons::ALL.map(AgentIcons::label).into();
        assert_eq!(labels, ["Large", "Small", "Off"]);
    }
}
