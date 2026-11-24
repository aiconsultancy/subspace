//! How the reads of the memberships ended, per account and organisation
//! scope.

use crate::{Error, model::MemberInitiative};

/// What the latest reads of the memberships settled.
#[derive(Default)]
pub(super) struct Outcome {
    /// Whether the latest read failed in transit or on the server.
    pub(super) failed: bool,
    /// Whether the latest read failed because the API could not be
    /// reached at all.
    pub(super) unreachable: bool,
    /// Whether the latest read was refused for want of a session, as before
    /// a saved one is restored at start.
    pub(super) sign_in_needed: bool,
    /// The account and organisation scope a read last succeeded for. A
    /// failed or refused read leaves who is an agent unknown again.
    pub(super) known: Option<Option<u64>>,
    /// The scope whose latest settled read found no memberships. A read that
    /// fails in transit settles nothing, so it leaves this as it was; a
    /// refusal, a missing endpoint or a missing organisation clears it.
    pub(super) empty: Option<Option<u64>>,
}

impl Outcome {
    /// Records how a read made for `scope` ended.
    pub(super) fn record(
        &mut self,
        scope: Option<u64>,
        result: &Result<Vec<MemberInitiative>, Error>,
    ) {
        self.failed = matches!(result, Err(error) if !settles(error));
        self.unreachable = matches!(result, Err(Error::Network));
        self.sign_in_needed = matches!(result, Err(Error::SignInNeeded));
        self.known = result.is_ok().then_some(scope);
        match result {
            Ok(items) => self.empty = items.is_empty().then_some(scope),
            Err(error) if settles(error) => self.empty = None,
            Err(_) => {}
        }
    }
}

/// Whether `error` is an answer about the memberships rather than a failure
/// to read them: an API without them, or a refusal.
fn settles(error: &Error) -> bool {
    matches!(
        error,
        Error::NotFound | Error::SignInNeeded | Error::Forbidden | Error::OrganisationNeeded
    )
}
