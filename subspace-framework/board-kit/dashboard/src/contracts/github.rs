//! Build GitHub navigation without knowing the responding board's identity.

use subspace_contracts::github::{OpenPullRequest, recognition::PullRequestLink};

use crate::BoardNavigation;

/// A requester declares `Declaration::requests::<OpenPullRequest>()`, then
/// emits `DashboardEvent::Navigate` with this value. With no running responder
/// the shell does nothing. Malformed or unsupported URLs produce no navigation.
// qual:api - Consumed by app crates through the Framework facade.
pub fn pull_request_navigation(url: &str) -> Option<BoardNavigation> {
    let link = PullRequestLink::parse(url)?;
    Some(BoardNavigation {
        responder: super::MessageKey::request::<OpenPullRequest>(),
        url: link.url().into(),
        origin: None,
    })
}
