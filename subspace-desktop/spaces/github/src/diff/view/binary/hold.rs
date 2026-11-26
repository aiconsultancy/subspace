//! Retry-After belongs to a pinned file side, not to its disposable reader.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use gpui_kit::{App, Context};
use subspace_client_fleet::diff::{DiffMode, DiffRead, DiffSide};

use super::{DiffView, Retry};
use crate::diff::view::pages::{BusyUntil, PageBusy};

#[derive(Default)]
struct Deadlines(Mutex<Vec<(String, DiffSide, BusyUntil)>>);

impl Deadlines {
    fn get(&self, path: &str, side: DiffSide) -> Option<BusyUntil> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|(held_path, held_side, _)| held_path == path && *held_side == side)
            .map(|(_, _, until)| *until)
    }

    fn record(&self, path: &str, side: DiffSide, now: Instant, seconds: u64) {
        let mut sides = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        sides.retain(|(_, _, until)| !until.delay(now).is_zero());
        let until = BusyUntil::new(now, seconds);
        if let Some((_, _, held)) = sides
            .iter_mut()
            .find(|(held_path, held_side, _)| held_path == path && *held_side == side)
        {
            if held.delay(now) < until.delay(now) {
                *held = until;
            }
        } else {
            sides.push((path.to_owned(), side, until));
        }
    }
}

#[derive(Default)]
pub(crate) struct BinaryHolds {
    pin: Option<(DiffMode, String)>,
    deadlines: Arc<Deadlines>,
    clocks: Vec<(String, DiffSide, PageBusy)>,
}

impl BinaryHolds {
    pub(crate) fn pin_to(&mut self, mode: DiffMode, at: &str) {
        if !self.is_pinned_to(mode, at) {
            self.pin = Some((mode, at.to_owned()));
            self.deadlines = Arc::default();
            self.clocks.clear();
        }
    }

    fn is_pinned_to(&self, mode: DiffMode, at: &str) -> bool {
        self.pin
            .as_ref()
            .is_some_and(|(held_mode, held_at)| *held_mode == mode && held_at == at)
    }

    pub(super) fn reader(&self, path: String, side: DiffSide, cx: &App) -> DiffRead {
        // A new pin replaces the store. Late replies can only update the
        // detached store for their captured mode and fingerprint.
        let deadlines = self.deadlines.clone();
        let executor = cx.background_executor().clone();
        DiffRead::default().observing_busy(move |seconds| {
            deadlines.record(&path, side, executor.now(), seconds);
        })
    }
}

impl DiffView {
    pub(super) fn binary_retry_remaining(&self, cx: &App) -> u64 {
        self.binary_deadline()
            .map_or(0, |until| until.remaining(cx.background_executor().now()))
    }

    fn binary_deadline(&self) -> Option<BusyUntil> {
        let (Some(binary), Some(at)) = (&self.binary, &self.fleet.fingerprint) else {
            return None;
        };
        if !self.binary_holds.is_pinned_to(self.fleet.mode, at) {
            return None;
        }
        let path = &self.files[binary.file].file.path;
        self.binary_holds.deadlines.get(path, binary.side)
    }

    pub(super) fn show_binary_retry(&mut self, retry: Retry, cx: &mut Context<Self>) {
        if let Some(binary) = self.binary.as_mut() {
            binary.retry = Some(retry);
        }
        self.start_binary_clock(cx);
    }

    pub(super) fn start_binary_clock(&mut self, cx: &mut Context<Self>) {
        let (Some(binary), Some(until)) = (&self.binary, self.binary_deadline()) else {
            return;
        };
        if until.remaining(cx.background_executor().now()) == 0 {
            return;
        }
        let path = &self.files[binary.file].file.path;
        let side = binary.side;
        self.binary_holds
            .clocks
            .retain(|(held_path, held_side, busy)| {
                !(held_path == path && *held_side == side) && busy.remaining(cx) > 0
            });
        self.binary_holds
            .clocks
            .push((path.clone(), side, PageBusy::at(until, cx)));
    }
}
