//! Seed selected crew data for inspector render tests without network I/O.

use gpui_kit::Context;
use subspace_client_fleet::{CrewDetail, Error, model::CrewMember};

use crate::Irc;

use super::{DetailState, Roster};

pub(crate) fn seed_roster_for_test(board: &mut Irc, member: CrewMember, cx: &mut Context<Irc>) {
    let selection = board.inspector_crew.as_mut().unwrap();
    selection.state = DetailState::new(Roster::Present(Box::new(member)), 0);
    cx.notify();
}

pub(crate) fn update_roster_for_test(board: &mut Irc, member: CrewMember, cx: &mut Context<Irc>) {
    let selection = board.inspector_crew.as_mut().unwrap();
    assert!(selection.state.update(Roster::Present(Box::new(member)), 0));
    cx.notify();
}

pub(crate) fn finish_crew_read_for_test(
    board: &mut Irc,
    result: Result<CrewDetail, Error>,
    cx: &mut Context<Irc>,
) {
    let selection = board.inspector_crew.as_mut().unwrap();
    selection.state.probe();
    let read = selection.state.begin().unwrap();
    selection.state.finish_read(read, result.map(Into::into));
    cx.notify();
}
