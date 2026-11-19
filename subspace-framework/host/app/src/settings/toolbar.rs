//! The application toolbar's draft presentation: per-control labels and the
//! collapsed menu. Save writes only the choices that changed since Settings
//! opened, so a concurrent context-menu label toggle is kept.

use gpui_kit::App;

use crate::toolbar_labels::{self, ToolbarControl};

#[derive(Clone, Copy, PartialEq)]
pub(super) struct ToolbarDraft {
    pub(super) labels: [bool; ToolbarControl::ALL.len()],
    pub(super) menu: bool,
}

impl ToolbarDraft {
    /// The current preferences, read fresh so draft and initial snapshots
    /// both reflect the values at open time.
    pub(super) fn load(cx: &App) -> Self {
        Self {
            labels: ToolbarControl::ALL.map(|control| control.shows_label(cx)),
            menu: toolbar_labels::collapsed(cx),
        }
    }

    pub(super) fn save(&self, initial: &Self, cx: &mut App) {
        for (index, control) in ToolbarControl::ALL.into_iter().enumerate() {
            if self.labels[index] != initial.labels[index] {
                control.set_label(self.labels[index], cx);
            }
        }
        if self.menu != initial.menu {
            toolbar_labels::set_collapsed(self.menu, cx);
        }
    }
}
