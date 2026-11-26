//! GitHub pull request navigation, independent of any board or account.
//!
//! A board declares that it responds to [`OpenPullRequest`]; requesters declare
//! that they request it. This request key is a navigation capability, routed
//! through `BoardNavigation` and the broker, not a synchronous data request.
//! The responder validates the URL again in `Dashboard::open_link`, opens its
//! normal detail panel and loads missing data with its own current account.
//! No credential, account choice or board ID crosses the contract.
//!
//! The same responder also resolves pull request references in text to
//! short summaries ([`ResolvePullRequests`]), with its own current account,
//! from memory: a reference it has not read yet is
//! [`ResolutionState::Pending`] and it publishes [`Changed`] when the read
//! lands. When the viewing account or its access changes, the responder
//! also publishes [`AccessChanged`] so subscribers can discard held summaries. Summaries are pull request content, so these
//! types do not implement `Debug`.

mod details;
pub mod recognition;

use crate::{Contract, Event, Request};
use recognition::PullRequestLink;

pub use details::{CheckCounts, CheckStatus, PullRequestDetails, PullRequestStatus, ReviewStatus};

/// At most this many pull requests in one [`ResolvePullRequests`].
pub const MAX_RESOLVE: usize = 50;

pub struct Github;

impl Contract for Github {
    const NAME: &'static str = "github";
    const VERSION: u32 = 1;
}

/// Navigation responder role for a URL recognised by [`recognition::PullRequestLink`].
/// Used as a `BoardNavigation` routing key; no `contracts::request` or attached
/// synchronous responder is needed. `Dashboard::open_link` accepts or refuses it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OpenPullRequest;

impl Request for OpenPullRequest {
    type Contract = Github;
    const NAME: &'static str = "open-pull-request";
    type Response = ();
}

/// Pull request summaries changed. Subscribers may ask for the latest answers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Changed;

impl Event for Changed {
    type Contract = Github;
    const NAME: &'static str = "changed";
}

/// The viewing account changed or its access was reset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AccessChanged;

impl Event for AccessChanged {
    type Contract = Github;
    const NAME: &'static str = "access-changed";
}

/// Asks for short summaries of pull requests by canonical
/// `owner/repository#number` key, read with the responder's current account.
#[derive(Clone, PartialEq, Eq)]
pub struct ResolvePullRequests {
    keys: Vec<String>,
}

impl ResolvePullRequests {
    /// Keeps the first [`MAX_RESOLVE`] distinct qualified keys, in order,
    /// written as [`PullRequestLink::key`] writes them. Others are dropped.
    pub fn new(keys: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        let mut distinct: Vec<String> = Vec::new();
        for link in keys
            .into_iter()
            .filter_map(|key| PullRequestLink::from_key(key.as_ref()))
        {
            if distinct.len() == MAX_RESOLVE {
                break;
            }
            let key = link.key();
            if !distinct.contains(&key) {
                distinct.push(key);
            }
        }
        Self { keys: distinct }
    }

    pub fn keys(&self) -> &[String] {
        &self.keys
    }
}

impl Request for ResolvePullRequests {
    type Contract = Github;
    const NAME: &'static str = "resolve-pull-requests";
    type Response = Vec<Resolution>;
}

/// What the responder knows about one pull request.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Resolution {
    pub key: String,
    pub state: ResolutionState,
}

impl Resolution {
    pub fn new(key: &str, state: ResolutionState) -> Self {
        Self {
            key: key.into(),
            state,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolutionState {
    Available(PullRequestDetails),
    /// The current account cannot read it. This never says whether it exists.
    Unavailable,
    /// A read has started; [`Changed`] follows when it lands.
    Pending,
    /// No GitHub account is signed in.
    SignedOut,
    /// The read failed for another reason, such as the network.
    Failed,
}
