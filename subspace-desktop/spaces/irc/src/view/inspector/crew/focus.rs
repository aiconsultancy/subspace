//! Restore focus when returning from a crew member to the Ship tab.

use gpui_kit::{Context, Window};
use subspace_client_fleet::FleetApi;

use crate::{Irc, live::CrewLine};

use super::{Roster, Selection, visible_members};

pub(super) fn close(board: &mut Irc, window: &mut Window, cx: &mut Context<Irc>) {
    let Some(selection) = board.inspector_crew.take() else {
        return;
    };
    let ship = super::super::front(board, cx).and_then(|front| front.fleet?.ship(&selection.ship));
    let focus = &mut board.inspector_crew_focus;
    let row = ship.as_ref().and_then(|ship| {
        FleetApi::current(cx)?;
        return_row(&selection, visible_members(ship, focus.show_all))
    });
    focus.returned = row.map(str::to_owned);
    if row.is_some() {
        focus.row.focus(window, cx);
    } else {
        focus.ship_tab.focus(window, cx);
    }
    cx.notify();
}

fn return_row<'a>(selection: &Selection, members: &'a [CrewLine]) -> Option<&'a str> {
    let mut rows = members.iter().filter(|member| {
        !matches!(selection.state.roster, Roster::Gone) || member.id != selection.member
    });
    let first = rows.next()?;
    if first.id == selection.member {
        return Some(&first.id);
    }
    rows.find(|member| member.id == selection.member)
        .map_or(Some(first.id.as_str()), |member| Some(member.id.as_str()))
}
