//! Issue references through the `issues` contract. IRC never names the board
//! that answers it: the hub asks for the scope when it starts and after each
//! `changed`, the transcript recognises references with that scope, and the
//! hub asks for summaries of the references on screen, the distinct ones a
//! frame shows in one request of at most 50. With no answering board, both
//! requests are unavailable, nothing is recognised and links stay links.
//! Scope and summaries live in memory only, never in IRC's store or search.

mod follow;
mod preview;
mod tile;
pub(crate) mod tooltip;

use std::collections::HashMap;

use subspace_framework::contracts::github;
use subspace_framework::contracts::issues::{
    Changed, CurrentScope, IssueScope, MAX_RESOLVE, Refresh, Resolution, ResolutionState, Resolve,
};
use subspace_framework::dashboard::contracts::{BoardContracts, ContractError, Declaration};

use crate::markup::IssueKeys;

pub(crate) use follow::{attach, open, refresh, want};
pub(crate) use preview::{panel, slot};
pub(crate) use tile::relative;
pub(crate) use tooltip::tooltip;

/// IRC's part in the `issues` contract, then in the `github` contract for
/// pull request references, declared at registration.
pub(crate) const CONTRACTS: BoardContracts = BoardContracts::new(&[
    Declaration::subscribes::<Changed>(),
    Declaration::requests::<CurrentScope>(),
    Declaration::requests::<Resolve>(),
    Declaration::requests::<Refresh>(),
    Declaration::subscribes::<github::Changed>(),
    Declaration::requests::<github::ResolvePullRequests>(),
    Declaration::requests::<github::OpenPullRequest>(),
]);

/// One identifier's last answer and the round it was given in.
struct Answer {
    state: ResolutionState,
    round: u64,
}

/// The scope references are recognised in and what the answering board said
/// about the ones shown.
#[derive(Default)]
pub(crate) struct IssueReferences {
    scope: Option<IssueScope>,
    keys: IssueKeys,
    answers: HashMap<String, Answer>,
    /// Shown identifiers to ask about, in the order they were shown.
    wanted: Vec<String>,
    /// Whether a request for `wanted` is already deferred.
    scheduled: bool,
    /// Counts `changed` events; an answer from an earlier round is asked again.
    round: u64,
    recognition_revision: u64,
}

impl IssueReferences {
    pub fn recognition_revision(&self) -> u64 {
        self.recognition_revision
    }

    /// The keys the transcript recognises references with.
    pub fn keys(&self) -> &IssueKeys {
        &self.keys
    }

    pub fn scope(&self) -> Option<&IssueScope> {
        self.scope.as_ref()
    }

    /// The answering board's profile name, for tooltips.
    pub fn profile_name(&self) -> Option<&str> {
        self.scope.as_ref().map(|scope| scope.profile_name.as_str())
    }

    /// What was last said about `identifier`; `None` before any answer.
    pub fn state(&self, identifier: &str) -> Option<&ResolutionState> {
        self.answers.get(identifier).map(|answer| &answer.state)
    }

    /// Whether `identifier` resolved as unavailable, which shows it muted.
    pub fn is_unavailable(&self, identifier: &str) -> bool {
        matches!(self.state(identifier), Some(ResolutionState::Unavailable))
    }

    /// Follows the answer to `current-scope`. Another profile, API origin
    /// or set of project keys forgets every answer. True when anything changed.
    pub fn set_scope(&mut self, scope: Option<IssueScope>) -> bool {
        if scope == self.scope {
            return false;
        }
        self.recognition_revision = self.recognition_revision.wrapping_add(1);
        self.keys = scope.as_ref().map(IssueKeys::new).unwrap_or_default();
        self.scope = scope;
        self.answers.clear();
        self.wanted.clear();
        true
    }

    /// After `changed`: every answer stays shown until it is asked again.
    pub fn start_round(&mut self) {
        self.round += 1;
    }

    /// Notes the identifiers a row shows. Those not yet asked this round are
    /// wanted; true when a request has to be scheduled for them.
    pub fn want(&mut self, identifiers: impl IntoIterator<Item = String>) -> bool {
        if self.scope.is_none() {
            return false;
        }
        for identifier in identifiers {
            let current = self
                .answers
                .get(&identifier)
                .is_some_and(|answer| answer.round == self.round);
            if !current && !self.wanted.contains(&identifier) {
                self.wanted.push(identifier);
            }
        }
        let schedule = !self.wanted.is_empty() && !self.scheduled;
        self.scheduled |= schedule;
        schedule
    }

    /// The wanted identifiers as requests of at most [`MAX_RESOLVE`] each.
    pub fn take_requests(&mut self) -> Vec<Resolve> {
        self.scheduled = false;
        let wanted = std::mem::take(&mut self.wanted);
        let Some(scope) = &self.scope else {
            return Vec::new();
        };
        wanted
            .chunks(MAX_RESOLVE)
            .map(|chunk| Resolve::new(&scope.profile_id, chunk.iter().cloned()))
            .collect()
    }

    /// Records `answers` in this round. True when what a row shows changed.
    pub fn record(&mut self, answers: Vec<Resolution>) -> bool {
        let mut changed = false;
        for resolution in answers {
            let previous = self.answers.insert(
                resolution.identifier,
                Answer {
                    state: resolution.state.clone(),
                    round: self.round,
                },
            );
            changed |= previous.is_none_or(|previous| previous.state != resolution.state);
        }
        changed
    }

    /// Asks `ask` for every wanted identifier and records the answers. A
    /// refusal means no board answers any more: nothing is recognised until
    /// the next `changed`. True when what a row shows changed.
    pub fn resolve_wanted(
        &mut self,
        mut ask: impl FnMut(&Resolve) -> Result<Vec<Resolution>, ContractError>,
    ) -> bool {
        let mut changed = false;
        for request in self.take_requests() {
            match ask(&request) {
                Ok(answers) => changed |= self.record(answers),
                Err(_) => return self.set_scope(None) || changed,
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests;
