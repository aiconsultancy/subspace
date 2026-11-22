//! The Icons choice: how large fleet agents' and ships' icons show, drafted
//! and saved like the icon button size.

use super::icon_buttons::Preference;
use gpui_kit::App;
use subspace_dashboard::avatar::{AGENT_ICONS_KEY, AgentIcons};

impl Preference for AgentIcons {
    const KEY: &'static str = AGENT_ICONS_KEY;
    const WHAT: &'static str = "icons";
    fn all() -> Vec<Self> {
        Self::ALL.into()
    }
    fn current(cx: &App) -> Self {
        Self::current(cx)
    }
    fn label(self) -> &'static str {
        self.label()
    }
    fn key(self) -> &'static str {
        self.key()
    }
    fn apply(self, cx: &mut App) {
        self.apply(cx);
    }
}
