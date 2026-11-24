//! One read of the memberships, and where the person's choice from it is
//! remembered.

use super::remembered;
use crate::{Client, Error, model::MemberInitiative};

/// One read of the memberships: the verified user it was made for, the
/// place their choice is remembered under (see [`remembered::place`]) and
/// what it found.
pub(super) struct Read {
    pub(super) actor: Option<String>,
    pub(super) place: Option<String>,
    pub(super) result: Result<Vec<MemberInitiative>, Error>,
}

impl Read {
    pub(super) fn failed(error: Error) -> Self {
        Self {
            actor: None,
            place: None,
            result: Err(error),
        }
    }

    /// A read a test supplies. Its scope stands in for the organisation, so
    /// each scope remembers its own choice.
    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn for_test(
        actor: Option<String>,
        scope: Option<u64>,
        result: Result<Vec<MemberInitiative>, Error>,
    ) -> Self {
        let organisation = scope.map(|scope| scope.to_string());
        Self {
            place: actor
                .as_deref()
                .map(|actor| remembered::place("", actor, organisation.as_deref())),
            actor,
            result,
        }
    }
}

/// The caller's memberships, with where they were read: the client's API
/// and, for NETT, the organisation every one of its requests names.
pub(super) fn memberships() -> Read {
    let client = match Client::new() {
        Ok(client) => client,
        Err(error) => return Read::failed(error),
    };
    match client.me() {
        Ok(actor) => Read {
            place: Some(remembered::place(
                client.api_origin(),
                &actor.user_id,
                client.organisation(),
            )),
            result: client.member_initiatives(),
            actor: Some(actor.user_id),
        },
        Err(error) => Read::failed(error),
    }
}
