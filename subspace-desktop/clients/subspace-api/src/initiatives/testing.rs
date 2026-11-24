//! The catalogue's test helpers, for this crate's tests and, through the
//! `test-support` feature, the Spaces' tests.

use gpui_kit::Context;

use super::{Catalog, Read};
use crate::{Error, model::MemberInitiative};

impl Catalog {
    /// Takes in `actor`'s memberships as a successful or failed read does,
    /// with the choice remembered for them in this scope.
    // qual:test_helper
    pub fn set_memberships_for_test(
        &mut self,
        actor: Option<String>,
        result: Result<Vec<MemberInitiative>, Error>,
        cx: &mut Context<Self>,
    ) {
        let read = Read::for_test(actor, self.scope, result);
        self.apply(read, cx);
    }

    /// Reads the memberships for `scope` with `read`, as an organisation
    /// switch's refresh does.
    // qual:test_helper
    pub fn refresh_for_test(
        &mut self,
        scope: Option<u64>,
        read: impl FnOnce() -> (Option<String>, Result<Vec<MemberInitiative>, Error>) + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let read = move || {
            let (actor, result) = read();
            Read::for_test(actor, scope, result)
        };
        self.refresh_using(scope, read, cx);
    }

    /// Moves the catalogue to another account or organisation scope, as an
    /// organisation switch does.
    // qual:test_helper
    pub fn change_scope_for_test(&mut self, scope: Option<u64>, cx: &mut Context<Self>) {
        if self.fence_scope(scope) {
            cx.notify();
        }
    }

    // qual:test_helper
    pub fn fleet_retry_pending_for_test(&self) -> bool {
        self.fleet_retry.task.is_some()
    }
}
