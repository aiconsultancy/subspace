//! Presentation preferences for the application toolbar. Each control
//! keeps an independent label choice, and one choice collapses every control
//! into the toolbar menu. All are scoped to the product's UI state file.
//! Context-menu and Settings callers share this one key mapping.

use gpui_kit::App;
use subspace_dashboard::prefs;

const MENU_KEY: &str = "app.toolbar-menu";

/// Whether the toolbar shows one menu instead of its controls. Default off;
/// only an explicit `"1"` turns it on.
pub fn collapsed(cx: &App) -> bool {
    prefs::get(cx, MENU_KEY).as_deref() == Some("1")
}

/// Stores the collapse choice in memory; Settings Save persists it.
pub fn set_collapsed(enabled: bool, cx: &mut App) {
    prefs::set(cx, MENU_KEY, if enabled { "1" } else { "0" });
}

/// How many toolbar controls this build has.
const CONTROLS: usize = if cfg!(feature = "report-issue") {
    11
} else {
    10
};

/// A toolbar control that can show an icon-only or icon-and-label treatment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarControl {
    Chat,
    /// Shown only for a product that docks a terminal.
    Terminal,
    Search,
    Commands,
    Reload,
    Wrap,
    Settings,
    Help,
    About,
    /// Only with the `report-issue` feature.
    #[cfg(feature = "report-issue")]
    ReportIssue,
    Jig,
}

impl ToolbarControl {
    /// Every configurable toolbar control, in Settings order.
    pub const ALL: [ToolbarControl; CONTROLS] = [
        Self::Chat,
        Self::Terminal,
        Self::Search,
        Self::Commands,
        Self::Reload,
        Self::Wrap,
        Self::Settings,
        Self::Help,
        Self::About,
        #[cfg(feature = "report-issue")]
        Self::ReportIssue,
        Self::Jig,
    ];

    /// The accessible name and Settings/context-menu label.
    pub fn label(self) -> &'static str {
        self.metadata().0
    }

    /// The stored product preference key for this control's label choice.
    pub fn key(self) -> &'static str {
        self.metadata().1
    }

    fn metadata(self) -> (&'static str, &'static str) {
        match self {
            Self::Chat => ("Chat", "app.toolbar-label.chat"),
            Self::Terminal => ("Terminal", "app.toolbar-label.terminal"),
            Self::Search => ("Search", "app.toolbar-label.search"),
            Self::Commands => ("Commands", "app.toolbar-label.commands"),
            Self::Reload => ("Reload", "app.toolbar-label.reload"),
            Self::Wrap => ("Wrap", "app.toolbar-label.wrap"),
            Self::Settings => ("Settings", "app.toolbar-label.settings"),
            Self::Help => ("Help", "app.toolbar-label.help"),
            Self::About => ("About", "app.toolbar-label.about"),
            #[cfg(feature = "report-issue")]
            Self::ReportIssue => ("Report issue", "app.toolbar-label.report-issue"),
            Self::Jig => ("Jig", "app.toolbar-label.jig"),
        }
    }

    /// Whether this app offers the control: the terminal needs a docked
    /// panel (`terminal`), Report issue a repository to file into.
    #[cfg_attr(not(feature = "report-issue"), allow(unused_variables))]
    pub fn offered(self, terminal: bool, cx: &App) -> bool {
        match self {
            Self::Terminal => terminal,
            #[cfg(feature = "report-issue")]
            Self::ReportIssue => crate::app::report_issue::available(cx),
            _ => true,
        }
    }

    /// Whether this control currently shows its label. Default off; only an
    /// explicit `"1"` turns it on.
    pub fn shows_label(self, cx: &App) -> bool {
        prefs::get(cx, self.key()).as_deref() == Some("1")
    }

    /// Stores this control's label choice in memory. Callers persist the
    /// change with `prefs::save` and refresh the window on their own
    /// schedule (Settings Save, or the context menu's immediate apply).
    pub fn set_label(self, enabled: bool, cx: &mut App) {
        prefs::set(cx, self.key(), if enabled { "1" } else { "0" });
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use gpui_kit::TestAppContext;
    use subspace_dashboard::prefs;

    use super::ToolbarControl;

    #[gpui_kit::test]
    fn labels_default_off_until_explicitly_enabled(cx: &mut TestAppContext) {
        cx.update(|cx| {
            prefs::init_file(None, cx);
            assert!(!ToolbarControl::Chat.shows_label(cx));
            ToolbarControl::Chat.set_label(true, cx);
            assert!(ToolbarControl::Chat.shows_label(cx));
            ToolbarControl::Chat.set_label(false, cx);
            assert!(!ToolbarControl::Chat.shows_label(cx));
        });
    }

    #[gpui_kit::test]
    fn each_control_keeps_its_own_preference(cx: &mut TestAppContext) {
        cx.update(|cx| {
            prefs::init_file(None, cx);
            ToolbarControl::Search.set_label(true, cx);
            assert!(ToolbarControl::Search.shows_label(cx));
            assert!(!ToolbarControl::Commands.shows_label(cx));
        });
    }

    #[gpui_kit::test]
    fn the_toolbar_menu_defaults_off_and_is_independent_of_labels(cx: &mut TestAppContext) {
        cx.update(|cx| {
            prefs::init_file(None, cx);
            assert!(!super::collapsed(cx));
            super::set_collapsed(true, cx);
            assert!(super::collapsed(cx));
            assert!(
                ToolbarControl::ALL
                    .iter()
                    .all(|control| !control.shows_label(cx))
            );
            super::set_collapsed(false, cx);
            assert!(!super::collapsed(cx));
        });
    }

    #[test]
    fn every_control_has_a_distinct_key_and_label() {
        let keys: HashSet<_> = ToolbarControl::ALL
            .iter()
            .map(|control| control.key())
            .collect();
        let labels: HashSet<_> = ToolbarControl::ALL
            .iter()
            .map(|control| control.label())
            .collect();
        assert_eq!(keys.len(), ToolbarControl::ALL.len());
        assert_eq!(labels.len(), ToolbarControl::ALL.len());
        assert!(keys.iter().all(|key| key.starts_with("app.toolbar-label.")));
        // Without the feature, Report issue leaves no control or setting.
        assert_eq!(
            labels.contains("Report issue"),
            cfg!(feature = "report-issue")
        );
    }
}
