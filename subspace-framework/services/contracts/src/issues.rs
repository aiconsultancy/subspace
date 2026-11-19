//! The `issues` contract: recognising issue references in text and resolving
//! them to short summaries, between a board that holds an issue tracker
//! session (the responder) and boards that show text mentioning issues.
//!
//! The responder answers both requests at once from memory. A reference it
//! has not resolved yet is [`ResolutionState::Pending`]: the responder starts
//! the read itself and publishes [`Changed`] when it lands. It also publishes
//! [`Changed`] when it starts and stops, and whenever the scope changes
//! (availability, API origin or project keys). Subscribers keep nothing
//! across a [`Changed`]: they ask again for the scope and for the references
//! they show. When no board responds, as when the Issues board is disabled,
//! both requests are unavailable and references show as plain text.
//!
//! Summaries are issue content, so these types do not implement `Debug`:
//! nothing logs them by accident. No message carries credentials.

pub mod recognition;

use crate::{Contract, Event, Request};

pub struct Issues;

impl Contract for Issues {
    const NAME: &'static str = "issues";
    const VERSION: u32 = 1;
}

/// At most this many identifiers in one [`Resolve`], as in a batch read.
pub const MAX_RESOLVE: usize = 50;

/// The scope changed or pending resolutions finished: ask again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Changed;

impl Event for Changed {
    type Contract = Issues;
    const NAME: &'static str = "changed";
}

/// Asks for the scope in which issue references are recognised now. The
/// answer is `None` while the responder is unavailable, and then nothing is
/// recognised.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CurrentScope;

impl Request for CurrentScope {
    type Contract = Issues;
    const NAME: &'static str = "current-scope";
    type Response = Option<IssueScope>;
}

/// The profile, API origin and project keys used to resolve references.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct IssueScope {
    pub profile_id: String,
    pub profile_name: String,
    pub api_origin: String,
    /// Loaded project keys, including archived projects.
    pub project_keys: Vec<String>,
}

impl IssueScope {
    pub fn new(profile_id: &str, profile_name: &str, api_origin: &str) -> Self {
        Self {
            profile_id: profile_id.into(),
            profile_name: profile_name.into(),
            api_origin: api_origin.into(),
            project_keys: Vec::new(),
        }
    }

    pub fn with_project_keys(mut self, keys: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.project_keys = keys.into_iter().map(Into::into).collect();
        self
    }

    /// Bind a navigation URL to the profile and API that resolved it.
    // qual:api - Consumed by app crates through the Framework facade.
    pub fn navigation_url(&self, url: &str) -> Option<String> {
        let (mut url, _) = self.parse_issue_url(url)?;
        url.query_pairs_mut()
            .clear()
            .append_pair("profile", &self.profile_id);
        Some(url.into())
    }

    /// Read a navigation target only within this exact API and profile scope.
    // qual:api - Consumed by app crates through the Framework facade.
    pub fn navigation_identifier(&self, url: &str) -> Option<String> {
        let (url, identifier) = self.parse_issue_url(url)?;
        let query: Vec<_> = url.query_pairs().collect();
        (query.len() == 1 && query[0].0 == "profile" && query[0].1 == self.profile_id)
            .then_some(identifier)
    }

    fn parse_issue_url(&self, url: &str) -> Option<(url::Url, String)> {
        let url = url::Url::parse(url).ok()?;
        let api = url::Url::parse(&self.api_origin).ok()?;
        if url.origin() != api.origin()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return None;
        }
        let prefix = format!("{}/v1/issues/", api.path().trim_end_matches('/'));
        let identifier = url.path().strip_prefix(&prefix)?;
        let keys =
            recognition::ReferenceRecognizer::new(self.project_keys.iter().map(String::as_str), []);
        if !keys.recognises(identifier) {
            return None;
        }
        let identifier = identifier.to_owned();
        Some((url, identifier))
    }
}

/// Asks for summaries of issue identifiers, such as `SUB-827`, in one
/// profile. The answer has one [`Resolution`] per identifier, in order. A
/// profile that is not the responder's current one resolves as unavailable,
/// never as another profile's issue.
#[derive(Clone, PartialEq, Eq)]
pub struct Resolve {
    profile_id: String,
    identifiers: Vec<String>,
}

impl Resolve {
    /// Keeps the first [`MAX_RESOLVE`] distinct identifiers, in order.
    pub fn new(profile_id: &str, identifiers: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let mut distinct: Vec<String> = Vec::new();
        for identifier in identifiers.into_iter().map(Into::into) {
            if distinct.len() == MAX_RESOLVE {
                break;
            }
            if !distinct.contains(&identifier) {
                distinct.push(identifier);
            }
        }
        Self {
            profile_id: profile_id.into(),
            identifiers: distinct,
        }
    }

    // qual:api
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    pub fn identifiers(&self) -> &[String] {
        &self.identifiers
    }
}

impl Request for Resolve {
    type Contract = Issues;
    const NAME: &'static str = "resolve";
    type Response = Vec<Resolution>;
}

/// What the responder knows about one identifier.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Resolution {
    pub identifier: String,
    pub state: ResolutionState,
}

impl Resolution {
    pub fn new(identifier: &str, state: ResolutionState) -> Self {
        Self {
            identifier: identifier.into(),
            state,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolutionState {
    Available(IssueSummary),
    /// The issue cannot be shown with this profile. This never says whether
    /// it exists.
    Unavailable,
    /// A read has started; [`Changed`] follows when it lands.
    Pending,
    SignedOut,
    /// The read failed for another reason, such as the network.
    Failed,
}

/// The workflow category of an issue's state, which picks its glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StateType {
    Triage,
    Backlog,
    Unstarted,
    Started,
    Completed,
    Canceled,
    Duplicate,
}

/// An issue's workflow state: its name and category.
// qual:api — adopted in phases 3 and 4 of the board-contracts plan.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct IssueState {
    pub name: String,
    pub kind: StateType,
}

impl IssueState {
    pub fn new(name: &str, kind: StateType) -> Self {
        Self {
            name: name.into(),
            kind,
        }
    }
}

mod summary;
pub use summary::{IssuePerson, IssueProgress, IssueSummary, LatestActivity, PullRequestSummary};
