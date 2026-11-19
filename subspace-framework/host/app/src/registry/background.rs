//! Process-owned board work, started and stopped with the active board set.

use gpui_kit::{App, Global};

use super::{BoardFactories, BoardRegistration};

type Hook = fn(&mut App);

/// The boards whose background work runs, each with the hook that stops it.
#[derive(Default)]
struct Running(Vec<(&'static str, Hook)>);
impl Global for Running {}

/// Starts the background work of each active board that does not run it
/// yet, and stops it for each board that is no longer active. Contract
/// routing changes between the two, so a stopping board can still announce
/// that it stops and a starting board can already announce that it starts.
pub(crate) fn sync_backgrounds(cx: &mut App) {
    let running = std::mem::take(&mut cx.default_global::<Running>().0);
    let active = &cx.global::<BoardFactories>().0;
    let change = change(&running, active);
    let contracts = declared_contracts(active);
    #[cfg(all(debug_assertions, feature = "capture"))]
    let contracts = crate::capture::issues::with_issues(contracts);
    cx.set_global(Running(change.running));
    change.stop.iter().for_each(|stop| stop(cx));
    subspace_dashboard::contracts::install(contracts, cx);
    change.start.iter().for_each(|start| start(cx));
}

/// Each board's ID with its contract declarations, in the boards' order.
pub(super) fn declared_contracts(
    boards: &[BoardRegistration],
) -> Vec<(&'static str, subspace_dashboard::contracts::BoardContracts)> {
    boards
        .iter()
        .map(|board| (board.id, board.contracts))
        .collect()
}

pub(super) struct Change {
    pub(super) start: Vec<Hook>,
    pub(super) stop: Vec<Hook>,
    pub(super) running: Vec<(&'static str, Hook)>,
}

pub(super) fn change(running: &[(&'static str, Hook)], active: &[BoardRegistration]) -> Change {
    let stop = running
        .iter()
        .filter(|(id, _)| !active.iter().any(|board| board.id == *id))
        .map(|(_, stop)| *stop)
        .collect();
    let start = active
        .iter()
        .filter(|board| !running.iter().any(|(id, _)| *id == board.id))
        .map(|board| board.start_background)
        .collect();
    let running = active
        .iter()
        .map(|board| (board.id, board.stop_background))
        .collect();
    Change {
        start,
        stop,
        running,
    }
}

#[cfg(test)]
mod contracts_tests;
