//! The right-click menu of a list row or a group header, built from the
//! standard groups: the row's standard commands, the board's own, and Fold
//! or Unfold for a header.

use std::rc::Rc;

use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{Context, Entity, SharedString, WeakEntity, Window};
use subspace_lucide::IconName;

use super::{ListDashboard, activate, update};
use crate::Callback;
use crate::row_menu::{self, GroupedMenu, MenuCommand, MenuGroup, RowMenu};

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
    let open = open_callback(entity.clone());
    move |menu, window, cx| {
        let _ = entity.update(cx, |this: &mut D, cx| {
            update(this, cx, |list| list.select(key.clone()));
        });
        let open = facts.opens_details.then(|| open.clone());
        let mut grouped = row_menu::grouped(row_menu::row_menu_items(&facts), open);
        let grouped = match entity.upgrade() {
            Some(board) => {
                if header {
                    let folded = board.read(cx).list().is_folded(&key);
                    grouped.push(fold_command(entity.clone(), key.clone(), folded));
                }
                board_commands(grouped, board, cx)
            }
            None => grouped,
        };
        row_menu::build(menu, grouped, window, cx)
    }
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
    cx: &mut Context<PopupMenu>,
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
