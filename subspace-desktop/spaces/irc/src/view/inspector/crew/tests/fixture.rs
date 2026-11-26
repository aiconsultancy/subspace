//! Seed selected crew data for inspector render tests without network I/O.

use gpui_kit::Context;
use subspace_client_fleet::{CrewDetail, model::CrewMember};

use crate::Irc;

use super::{DetailState, Roster};

pub(crate) fn seed_roster_for_test(board: &mut Irc, member: CrewMember, cx: &mut Context<Irc>) {
    let selection = board.inspector_crew.as_mut().unwrap();
    selection.state = DetailState::new(Roster::Present(Box::new(member)), 0);
    cx.notify();
}

pub(crate) fn finish_crew_read_for_test(
    board: &mut Irc,
    detail: CrewDetail,
    cx: &mut Context<Irc>,
) {
    let selection = board.inspector_crew.as_mut().unwrap();
    selection.state.probe();
    let read = selection.state.begin().unwrap();
    selection.state.finish_read(read, Ok(detail.into()));
    cx.notify();
}
