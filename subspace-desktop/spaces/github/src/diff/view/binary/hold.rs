//! Retry-After belongs to a pinned file side, not to its disposable reader.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use gpui_kit::base::async_util::{Receiver, Sender, unbounded};
use gpui_kit::{App, Context, Task};
use subspace_client_fleet::diff::{DiffMode, DiffRead, DiffSide};

use super::{DiffView, Retry};
use crate::diff::view::pages::{BusyUntil, countdown};

#[derive(Default)]
struct Deadlines {
    sides: Mutex<Vec<(String, DiffSide, BusyUntil)>>,
    visible: Mutex<Option<VisibleSide>>,
}

struct VisibleSide {
    path: String,
    side: DiffSide,
    changed: Sender<()>,
}

impl Deadlines {
    fn get(&self, path: &str, side: DiffSide) -> Option<BusyUntil> {
        self.sides
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|(held_path, held_side, _)| held_path == path && *held_side == side)
            .map(|(_, _, until)| *until)
    }

    fn record(&self, path: &str, side: DiffSide, now: Instant, seconds: u64) {
        if self.extend(path, side, now, seconds)
            && let Some(visible) = &*self.visible.lock().unwrap_or_else(PoisonError::into_inner)
            && visible.path == path
            && visible.side == side
        {
            _ = visible.changed.try_send(());
        }
    }

    fn extend(&self, path: &str, side: DiffSide, now: Instant, seconds: u64) -> bool {
        let mut sides = self.sides.lock().unwrap_or_else(PoisonError::into_inner);
        sides.retain(|(_, _, until)| !until.delay(now).is_zero());
        let until = BusyUntil::new(now, seconds);
        if let Some((_, _, held)) = sides
            .iter_mut()
            .find(|(held_path, held_side, _)| held_path == path && *held_side == side)
        {
            if held.delay(now) >= until.delay(now) {
                return false;
            }
            *held = until;
        } else {
            sides.push((path.to_owned(), side, until));
        }
        true
    }

    fn watch(&self, path: String, side: DiffSide) -> Receiver<()> {
        let (changed, receiver) = unbounded();
        *self.visible.lock().unwrap_or_else(PoisonError::into_inner) = Some(VisibleSide {
            path,
            side,
            changed,
        });
        receiver
    }
}

#[derive(Default)]
pub(crate) struct BinaryHolds {
    pin: Option<(DiffMode, String)>,
    deadlines: Arc<Deadlines>,
}

impl BinaryHolds {
    pub(crate) fn pin_to(&mut self, mode: DiffMode, at: &str) {
        if !self.is_pinned_to(mode, at) {
            self.pin = Some((mode, at.to_owned()));
            self.deadlines = Arc::default();
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
        let (recorded, clock, held_path) = (deadlines.clone(), executor.clone(), path.clone());
        DiffRead::default()
            .observing_busy(move |seconds| recorded.record(&held_path, side, clock.now(), seconds))
            .respecting_hold(move || {
                let deadline = deadlines.get(&path, side);
                let now = executor.now();
                (
                    now,
                    deadline.map_or(Duration::ZERO, |until| until.delay(now)),
                )
            })
    }

    pub(super) fn watch(
        &self,
        path: String,
        side: DiffSide,
        cx: &mut Context<DiffView>,
    ) -> Task<()> {
        let changed = self.deadlines.watch(path, side);
        cx.spawn(async move |view, cx| {
            while changed.recv().await.is_ok() {
                if view.update(cx, DiffView::binary_hold_changed).is_err() {
                    break;
                }
            }
        })
    }
}

impl DiffView {
    fn binary_hold_changed(&mut self, cx: &mut Context<Self>) {
        if !self.fleet.changed() && self.binary_retry_remaining(cx) > 0 {
            self.start_binary_clock(cx);
            cx.notify();
        }
    }

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
        let Some(binary) = &mut self.binary else {
            return;
        };
        let path = self.files[binary.file].file.path.clone();
        let side = binary.side;
        let deadlines = self.binary_holds.deadlines.clone();
        // Only the visible reader owns a timer. Its passive hold survives
        // close, and a late response can extend the currently visible hold.
        binary.clock = countdown(cx, move |now| {
            deadlines
                .get(&path, side)
                .map_or(Duration::ZERO, |until| until.delay(now))
        });
    }
}
