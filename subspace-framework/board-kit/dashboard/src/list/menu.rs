//! The right-click menu of a list row or a group header, built from the
//! standard groups: the row's standard commands, the board's own, and Fold
//! or Unfold for a header.

use std::rc::Rc;

use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{App, Context, Entity, SharedString, WeakEntity, Window};
use subspace_lucide::IconName;

use super::{ListDashboard, activate, update};
use crate::Callback;
use crate::row_menu::{self, GroupedMenu, MenuCommand, MenuGroup, RowMenu};

/// Why a menu opened over a line the board would not select, because a
/// dialog of the board's is still open, cannot act on that line.
const BLOCKED_REASON: &str = "Finish the open dialog first.";

/// The menu a row or group header (`header`) opens on a right click.
/// Opening it selects the line first, so the shell's Add to chat acts on
/// the line the menu was opened over, and so do the board's own commands.
pub(crate) fn line_menu<D: ListDashboard>(
    key: SharedString,
    facts: RowMenu,
    header: bool,
    cx: &mut Context<D>,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    let entity = cx.entity().downgrade();
    move |menu, window, cx| {
        let grouped = line_commands(&entity, &key, &facts, header, cx);
        row_menu::build(menu, grouped, window, cx)
    }
}

/// The commands of the menu for line `key`, after selecting it. A board
/// that blocks navigation (`ListDashboard::can_navigate`) keeps its
/// selection on another line; the menu then offers only what the line's own
/// facts can do, and shows the commands that act on the selection disabled,
/// rather than building them for the line that is still selected.
pub(super) fn line_commands<D: ListDashboard>(
    entity: &WeakEntity<D>,
    key: &SharedString,
    facts: &RowMenu,
    header: bool,
    cx: &mut App,
) -> GroupedMenu {
    let reason = SharedString::new_static(BLOCKED_REASON);
    let items = row_menu::row_menu_items(facts);
    let Some(board) = entity.upgrade() else {
        return row_menu::grouped_unselected(items, &reason);
    };
    match select_line(&board, key, cx) {
        true => selected_line_commands(board, key, facts, header, cx),
        false => {
            let grouped = row_menu::grouped_unselected(items, &reason);
            with_blocked_fold(grouped, &board, key, header, cx)
        }
    }
}

/// Selects `key`, as opening its menu does, and says whether it is now the
/// selected line.
fn select_line<D: ListDashboard>(board: &Entity<D>, key: &SharedString, cx: &mut App) -> bool {
    board.update(cx, |this, cx| {
        update(this, cx, |list| list.select(key.clone()));
        this.list().selected() == Some(key)
    })
}

/// The full menu for the selected line: its standard commands, Fold or
/// Unfold for a header, and the board's own commands.
fn selected_line_commands<D: ListDashboard>(
    board: Entity<D>,
    key: &SharedString,
    facts: &RowMenu,
    header: bool,
    cx: &mut App,
) -> GroupedMenu {
    let entity = board.downgrade();
    let open = facts.opens_details.then(|| open_callback(entity.clone()));
    let mut grouped = row_menu::grouped(row_menu::row_menu_items(facts), open);
    if header {
        let folded = board.read(cx).list().is_folded(key);
        grouped.push(fold_command(entity, key.clone(), folded));
    }
    board_commands(grouped, board, cx)
}

/// A header's Fold or Unfold, disabled, for a menu opened over a header the
/// board would not select. The board's own commands are left out of such a
/// menu, since they read the selection to find their line.
fn with_blocked_fold<D: ListDashboard>(
    mut grouped: GroupedMenu,
    board: &Entity<D>,
    key: &SharedString,
    header: bool,
    cx: &App,
) -> GroupedMenu {
    if header {
        let folded = board.read(cx).list().is_folded(key);
        let fold = fold_command(board.downgrade(), key.clone(), folded);
        grouped.push(fold.disabled(true, BLOCKED_REASON));
    }
    grouped
}

/// The row's Enter action.
fn open_callback<D: ListDashboard>(entity: WeakEntity<D>) -> Callback {
    let open: Callback = Rc::new(move |_, cx| {
        let _ = entity.update(cx, |this: &mut D, cx| activate(this, cx));
    });
    open
}

/// Adds the board's own commands, and what its older hooks add straight to
/// the menu at the end of the Act group.
fn board_commands<D: ListDashboard>(
    mut grouped: GroupedMenu,
    board: Entity<D>,
    cx: &App,
) -> GroupedMenu {
    grouped.extend(D::row_commands(&board, cx));
    grouped.extend_act(Box::new(move |menu, window, cx| {
        let menu = board.read(cx).extend_row_menu(menu, cx);
        D::row_submenus(&board, menu, window, cx)
    }))
}

/// Fold group or Unfold group, for the header of `group`.
fn fold_command<D: ListDashboard>(
    entity: WeakEntity<D>,
    group: SharedString,
    folded: bool,
) -> MenuCommand {
    let (label, icon) = match folded {
        true => ("Unfold group", IconName::ChevronsUpDown),
        false => ("Fold group", IconName::ChevronsDownUp),
    };
    MenuCommand::new(MenuGroup::View, label, icon, move |_, cx| {
        let group = group.clone();
        let _ = entity.update(cx, |this: &mut D, cx| {
            update(this, cx, |list| list.toggle(&group));
        });
    })
}

#[cfg(test)]
mod tests;
