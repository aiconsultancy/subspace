//! A Space's right-click menu, on its switcher tab and its navigation rail
//! destination: close the Space, or open its Settings page. Closing saves
//! and applies the same choice as turning the Space off in Settings.

use gpui_kit::component::menu::{ContextMenu, PopupMenu, PopupMenuItem};
use gpui_kit::{
    Context, InteractiveElement, ParentElement, SharedString, Styled, WeakEntity, Window,
};
use subspace_dashboard::Toast;
use subspace_dashboard::anchored::AnchoredContextMenuExt as _;
use subspace_dashboard::prefs;

use super::{ACTIVE_KEY, SubspaceApp};
use crate::settings::board_choice::{self, Applied};

/// The menu's wording.
pub(super) const CLOSE_SPACE: &str = "Close Space";
pub(super) const SPACE_SETTINGS: &str = "Space settings…";
pub(super) const LAST_SPACE: &str = "The last open Space can't be closed.";

/// Adds Space `id`'s right-click menu to its tab. `closable` is false for
/// the last open Space, whose Close item is disabled with the reason.
pub(super) fn with_space_menu<E>(
    element: E,
    id: &'static str,
    closable: bool,
    app: WeakEntity<SubspaceApp>,
) -> ContextMenu<E>
where
    E: InteractiveElement + ParentElement + Styled,
{
    element.anchored_context_menu(move |menu, _, _| space_menu(menu, id, closable, &app))
}

fn space_menu(
    menu: PopupMenu,
    id: &'static str,
    closable: bool,
    app: &WeakEntity<SubspaceApp>,
) -> PopupMenu {
    let close_app = app.clone();
    let settings_app = app.clone();
    let menu = menu.item(
        PopupMenuItem::new(CLOSE_SPACE)
            .disabled(!closable)
            .on_click(move |_, window, cx| {
                _ = close_app.update(cx, |app, cx| app.close_space(id, window, cx));
            }),
    );
    let menu = if closable {
        menu
    } else {
        menu.label(LAST_SPACE)
    };
    menu.separator().item(
        PopupMenuItem::new(SPACE_SETTINGS).on_click(move |_, window, cx| {
            _ = settings_app.update(cx, |app, cx| app.open_board_settings(id, window, cx));
        }),
    )
}

/// The tab that takes over from the tab at `index` when it closes: the one
/// before it, or the next one when it is first. None for the last tab.
pub(super) fn neighbour(index: usize, len: usize) -> Option<usize> {
    if len < 2 || index >= len {
        None
    } else if index == 0 {
        Some(1)
    } else {
        Some(index - 1)
    }
}

/// The toast after Space `title` closed, saying where to reopen it.
fn closed_message(title: &str, applied: Applied) -> String {
    match applied {
        Applied::Now => format!("Closed {title}. Turn it on in Settings to reopen it."),
        Applied::AfterRestart => format!("Restart the app to close {title}."),
    }
}

impl SubspaceApp {
    /// Whether a Space can be closed from its tab: never the last one.
    pub(super) fn can_close_space(&self) -> bool {
        self.dashboards.len() > 1
    }

    /// Turns Space `id` off as its Settings switch and Save do. When it was
    /// the current Space, its neighbour becomes current and takes focus.
    pub(super) fn close_space(
        &mut self,
        id: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.dashboards.iter().position(|board| board.id == id) else {
            return;
        };
        let Some(next) = neighbour(index, self.dashboards.len()) else {
            return;
        };
        let title = self.dashboards[index].title.clone();
        let next = self.dashboards[next].id;
        let was_active = index == self.active;
        match board_choice::save_board_enabled(id, false, window, cx) {
            Ok(applied) => {
                self.closed_space(&title, applied, was_active.then_some(next), window, cx)
            }
            Err(error) => {
                self.push_toast(Toast::error(format!("Couldn't close {title}: {error}")), cx)
            }
        }
    }

    fn closed_space(
        &mut self,
        title: &SharedString,
        applied: Applied,
        next: Option<&'static str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let (Applied::Now, Some(next)) = (applied, next) {
            self.follow_neighbour(next, window, cx);
        }
        self.push_toast(Toast::info(closed_message(title, applied)), cx);
    }

    /// Makes Space `next` current after the current one closed. While Logs or
    /// Search replaces the board, it stays shown and `next` waits behind it.
    fn follow_neighbour(
        &mut self,
        next: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.dashboards.iter().position(|board| board.id == next) else {
            return;
        };
        if self.main.is_board() {
            self.switch_to(index, window, cx);
        } else {
            self.active = index;
            prefs::set(cx, ACTIVE_KEY, next);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Applied, closed_message, neighbour};

    #[test]
    fn the_neighbour_is_the_previous_tab_or_the_next_for_the_first() {
        assert_eq!(neighbour(2, 3), Some(1));
        assert_eq!(neighbour(1, 3), Some(0));
        assert_eq!(neighbour(0, 3), Some(1));
        assert_eq!(neighbour(0, 1), None);
        assert_eq!(neighbour(3, 3), None);
    }

    #[test]
    fn the_closed_toast_says_where_to_reopen_the_space() {
        assert_eq!(
            closed_message("Actions", Applied::Now),
            "Closed Actions. Turn it on in Settings to reopen it."
        );
        assert_eq!(
            closed_message("Actions", Applied::AfterRestart),
            "Restart the app to close Actions."
        );
    }
}
