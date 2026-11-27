//! Connection changes and explicit refusals invalidate the visible snapshot.

use gpui_kit::Context;
use subspace_client_fleet::{Error, FleetAccess};

use super::read_executor::diff_error;
use super::{DiffSource, DiffView};

impl DiffView {
    pub(super) fn set_access(
        &mut self,
        access: Option<FleetAccess>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.fleet.access == access {
            return false;
        }
        self.cancel_list();
        self.clear_snapshot(cx);
        self.fleet.access = access;
        true
    }

    pub(super) fn check_crew_access(&mut self, cx: &mut Context<Self>) -> bool {
        let DiffSource::Fleet(source) = &self.source else {
            return false;
        };
        if self.fleet.access.is_none() {
            return false;
        }
        let (access, error) = match source.connection(cx) {
            Ok((_, access)) => (Some(access), None),
            Err(help) => (None, Some(help)),
        };
        if !self.set_access(access, cx) {
            return false;
        }
        self.error = Some(error.unwrap_or_else(|| {
            "The Fleet connection changed. Refresh to load the diff.".to_owned()
        }));
        true
    }

    /// An authenticated read was explicitly refused. Only call this after
    /// checking that the read still belongs to the current view and access.
    pub(super) fn reject_crew_access(&mut self, error: Error, cx: &mut Context<Self>) -> bool {
        if !matches!(error, Error::TokenRefused | Error::Forbidden) {
            return false;
        }
        self.set_access(None, cx);
        self.error = Some(diff_error(error));
        true
    }
}
