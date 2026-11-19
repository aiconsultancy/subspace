//! What the board area shows: the active board, or one Framework view in
//! its place. The board is inactive exactly while a view replaces it.

use gpui_kit::{AnyView, App, Entity, FocusHandle, Focusable as _, Subscription, Window};
use subspace_dashboard::log_view::LogView;
use subspace_search_ui::SearchView;

use super::SubspaceApp;

#[cfg(test)]
mod tests;

pub(super) enum MainSurface {
    Board,
    Search {
        view: Entity<SearchView>,
        _subscription: Subscription,
    },
    Logs {
        view: Entity<LogView>,
        _subscription: Subscription,
    },
}

impl MainSurface {
    pub(super) fn is_board(&self) -> bool {
        matches!(self, Self::Board)
    }

    pub(super) fn is_logs(&self) -> bool {
        matches!(self, Self::Logs { .. })
    }

    pub(super) fn search(&self) -> Option<&Entity<SearchView>> {
        match self {
            Self::Search { view, .. } => Some(view),
            Self::Board | Self::Logs { .. } => None,
        }
    }

    /// The window title's view name, when a Framework view replaces the board.
    pub(super) fn title(&self) -> Option<&'static str> {
        match self {
            Self::Board => None,
            Self::Search { .. } => Some("Search"),
            Self::Logs { .. } => Some("Logs"),
        }
    }

    pub(super) fn view(&self) -> Option<AnyView> {
        match self {
            Self::Board => None,
            Self::Search { view, .. } => Some(view.clone().into()),
            Self::Logs { view, .. } => Some(view.clone().into()),
        }
    }

    pub(super) fn focus_handle(&self, cx: &App) -> Option<FocusHandle> {
        match self {
            Self::Board => None,
            Self::Search { view, .. } => Some(view.focus_handle(cx)),
            Self::Logs { view, .. } => Some(view.focus_handle(cx)),
        }
    }
}

impl SubspaceApp {
    /// Whether the tabs or rail show board `index` as current. None is
    /// current while a Framework view replaces the board.
    pub(super) fn shows_current(&self, index: usize) -> bool {
        index == self.active && self.main.is_board()
    }

    /// Show `next` in the board area. The board deactivates once when a view
    /// replaces it and activates once when it returns; replacing one view
    /// with another leaves the board alone.
    pub(super) fn set_main(&mut self, next: MainSurface, window: &mut Window, cx: &mut App) {
        self.remember_view(cx);
        let was_board = std::mem::replace(&mut self.main, next).is_board();
        let Some(board) = self.dashboard() else {
            return;
        };
        match (was_board, self.main.is_board()) {
            (true, false) => board.deactivate(window, cx),
            (false, true) => board.activate(window, cx),
            (true, true) | (false, false) => {}
        }
    }

    /// Drop any Framework view without activating the board, for callers
    /// that switch, reveal or route the board themselves. Returns whether
    /// the board was already showing, and so is still active.
    pub(super) fn leave_main(&mut self) -> bool {
        std::mem::replace(&mut self.main, MainSurface::Board).is_board()
    }
}
