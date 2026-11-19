//! The last view: each time the shell changes the current board it tells
//! the board it shows which view the person was on, so a Back there can
//! return to it ([`DashboardEvent::Return`]).
//!
//! [`DashboardEvent::Return`]: subspace_dashboard::DashboardEvent::Return

use gpui_kit::{App, Context, Window};
use subspace_dashboard::{ReturnLocation, ReturnPoint, Toast};

use super::SubspaceApp;

impl SubspaceApp {
    /// The board view showing now, as a place to return to: at `place` when
    /// the board named it itself, such as the conversation a link was
    /// followed from, otherwise where the board says it is. `None` while a
    /// Framework view, such as Search, replaces the board.
    fn current_view_at(&self, place: Option<ReturnLocation>, cx: &App) -> Option<ReturnPoint> {
        if !self.main.is_board() {
            return None;
        }
        let board = self.dashboard()?;
        let location = place.or_else(|| board.return_location(cx));
        Some(ReturnPoint {
            board: board.id,
            label: location
                .as_ref()
                .map_or_else(|| board.title.clone(), |place| place.label.clone()),
            location: location.map(|place| place.key),
        })
    }

    /// Remembers the board view showing now, before a Framework view such
    /// as Search replaces it.
    pub(super) fn remember_view(&mut self, cx: &App) {
        self.remember_view_at(None, cx);
    }

    fn remember_view_at(&mut self, place: Option<ReturnLocation>, cx: &App) {
        if let Some(view) = self.current_view_at(place, cx) {
            self.last_view = Some(view);
        }
    }

    /// Tells board `index` the view it is reached from, before the shell
    /// shows it: the board view showing now or, under a Framework view, the
    /// last one. Showing the board already current records nothing.
    pub(super) fn note_arrival(&mut self, index: usize, cx: &mut App) {
        self.note_arrival_from(index, None, cx);
    }

    /// [`Self::note_arrival`] from `place` in the current board, when the
    /// board named the place a link was followed from.
    pub(super) fn note_arrival_from(
        &mut self,
        index: usize,
        place: Option<ReturnLocation>,
        cx: &mut App,
    ) {
        if self.shows_current(index) && self.dashboard().is_some() {
            return;
        }
        self.remember_view_at(place, cx);
        let from = self.last_view.clone().filter(|view| {
            self.dashboards
                .get(index)
                .is_none_or(|b| b.id != view.board)
        });
        if let Some(board) = self.dashboards.get(index) {
            board.arrived_from(from, cx);
        }
    }

    /// [`Self::note_arrival`] for the board with id `id`, if it is open.
    pub(super) fn note_arrival_at(&mut self, id: &str, cx: &mut App) {
        if let Some(index) = self.dashboards.iter().position(|board| board.id == id) {
            self.note_arrival(index, cx);
        }
    }

    /// A Back on the current board asks for `point`. When its board or the
    /// place in it no longer exists, the requester stays, showing its own
    /// fallback, and a toast says why.
    pub(super) fn return_to_view(
        &mut self,
        requester: &'static str,
        point: &ReturnPoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.shows_current(self.active)
            || self.dashboard().is_none_or(|board| board.id != requester)
            || point.board == requester
        {
            return;
        }
        let Some(index) = self.dashboards.iter().position(|b| b.id == point.board) else {
            self.push_toast(Toast::info(format!("{} is closed.", point.label)), cx);
            return;
        };
        if let Some(key) = &point.location
            && !self.dashboards[index].return_to(key, window, cx)
        {
            self.push_toast(
                Toast::info(format!("{} is no longer open.", point.label)),
                cx,
            );
            return;
        }
        self.switch_to(index, window, cx);
    }
}

#[cfg(test)]
#[path = "last_view_tests.rs"]
mod tests;
