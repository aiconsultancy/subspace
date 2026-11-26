//! Retry-After belongs to a pinned file side, not to its disposable reader.

use gpui_kit::{App, Context};
use subspace_client_fleet::diff::{DiffMode, DiffSide};

use super::{DiffView, Retry};
use crate::diff::view::pages::PageBusy;

#[derive(Default)]
pub(crate) struct BinaryHolds {
    pin: Option<(DiffMode, String)>,
    sides: Vec<(String, DiffSide, PageBusy)>,
}

impl BinaryHolds {
    pub(crate) fn pin_to(&mut self, mode: DiffMode, at: &str) {
        if !self.is_pinned_to(mode, at) {
            self.pin = Some((mode, at.to_owned()));
            self.sides.clear();
        }
    }

    fn is_pinned_to(&self, mode: DiffMode, at: &str) -> bool {
        self.pin
            .as_ref()
            .is_some_and(|(held_mode, held_at)| *held_mode == mode && held_at == at)
    }

    fn remaining(&self, path: &str, side: DiffSide, cx: &App) -> u64 {
        self.sides
            .iter()
            .find(|(held_path, held_side, _)| held_path == path && *held_side == side)
            .map_or(0, |(_, _, busy)| busy.remaining(cx))
    }
}

impl DiffView {
    pub(super) fn binary_retry_remaining(&self, cx: &App) -> u64 {
        let (Some(binary), Some(at)) = (&self.binary, &self.fleet.fingerprint) else {
            return 0;
        };
        if !self.binary_holds.is_pinned_to(self.fleet.mode, at) {
            return 0;
        }
        let path = &self.files[binary.file].file.path;
        self.binary_holds.remaining(path, binary.side, cx)
    }

    pub(super) fn hold_binary_retry(
        &mut self,
        retry: Retry,
        wait: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        let Some(binary) = self.binary.as_mut() else {
            return;
        };
        binary.retry = Some(retry);
        let (Some(at), Some(wait @ 1..)) = (&self.fleet.fingerprint, wait) else {
            return;
        };
        self.binary_holds.pin_to(self.fleet.mode, at);
        let path = &self.files[binary.file].file.path;
        let side = binary.side;
        self.binary_holds
            .sides
            .retain(|(held_path, held_side, busy)| {
                !(held_path == path && *held_side == side) && busy.remaining(cx) > 0
            });
        self.binary_holds
            .sides
            .push((path.clone(), side, PageBusy::new(Some(wait), cx)));
    }
}
