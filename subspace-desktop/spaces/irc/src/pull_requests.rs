//! GitHub pull request references through the `github` contract.
//! IRC never names the Space that answers: the hub asks for short summaries
//! of the pull requests on screen, the distinct ones a frame shows in one
//! request of at most 50, keyed by canonical `owner/repo#n`. The answering
//! Space reads them with its own current account. On `changed` (another
//! account, a sign-out, or reads that landed) every answer is forgotten, so
//! no detail outlives the account that could read it. With no answering
//! Space the chips stay and say their details are unavailable. Answers live
//! in memory only, never in IRC's store or search.

mod compact;
mod follow;
mod slot;
mod tile;
mod tooltip;

use std::collections::{HashMap, HashSet};

use subspace_framework::contracts::github::{
    MAX_RESOLVE, Resolution, ResolutionState, ResolvePullRequests,
};
use subspace_framework::dashboard::contracts::ContractError;

pub(crate) use follow::{attach, open, want};
pub(crate) use slot::slot;
pub(crate) use tooltip::tooltip;

/// What the answering Space said about the pull requests shown, since the
/// last `changed`.
#[derive(Default)]
pub(crate) struct PullRequestReferences {
    answers: HashMap<String, ResolutionState>,
    stale: HashSet<String>,
    /// Shown keys to ask about, in the order they were shown.
    wanted: Vec<String>,
    /// Whether a request for `wanted` is already deferred.
    scheduled: bool,
}

impl PullRequestReferences {
    /// What was last said about canonical `key`; `None` before any answer.
    pub fn state(&self, key: &str) -> Option<&ResolutionState> {
        self.answers.get(key)
    }

    /// After `changed`: forget every answer, so each is asked again.
    pub fn changed(&mut self) {
        self.answers.clear();
        self.stale.clear();
        self.wanted.clear();
    }

    /// Revalidate summaries without dropping the contents of existing tiles.
    pub fn refreshed(&mut self) {
        self.stale = self.answers.keys().cloned().collect();
    }

    /// Notes the keys a row shows. Those not yet answered are wanted; true
    /// when a request has to be scheduled for them.
    pub fn want(&mut self, keys: impl IntoIterator<Item = String>) -> bool {
        for key in keys {
            if (!self.answers.contains_key(&key) || self.stale.contains(&key))
                && !self.wanted.contains(&key)
            {
                self.wanted.push(key);
            }
        }
        let schedule = !self.wanted.is_empty() && !self.scheduled;
        self.scheduled |= schedule;
        schedule
    }

    /// The wanted keys as requests of at most [`MAX_RESOLVE`] each.
    pub fn take_requests(&mut self) -> Vec<ResolvePullRequests> {
        self.scheduled = false;
        std::mem::take(&mut self.wanted)
            .chunks(MAX_RESOLVE)
            .map(|chunk| ResolvePullRequests::new(chunk.iter()))
            .collect()
    }

    /// Records `answers` by canonical key. True when what a row shows changed.
    pub fn record(&mut self, answers: Vec<Resolution>) -> bool {
        let mut changed = false;
        for resolution in answers {
            let key = resolution.key.to_ascii_lowercase();
            self.stale.remove(&key);
            if matches!(
                resolution.state,
                ResolutionState::Pending | ResolutionState::Failed
            ) && matches!(self.answers.get(&key), Some(ResolutionState::Available(_)))
            {
                continue;
            }
            let previous = self.answers.insert(key, resolution.state.clone());
            changed |= previous.is_none_or(|previous| previous != resolution.state);
        }
        changed
    }

    /// Asks `ask` for every wanted key and records the answers. A refusal
    /// means no Space answers: those chips have no details until `changed`.
    pub fn resolve_wanted(
        &mut self,
        mut ask: impl FnMut(&ResolvePullRequests) -> Result<Vec<Resolution>, ContractError>,
    ) -> bool {
        let mut changed = false;
        for request in self.take_requests() {
            let answers = ask(&request).unwrap_or_else(|_| unavailable(request.keys()));
            changed |= self.record(answers);
        }
        changed
    }
}

fn unavailable(keys: &[String]) -> Vec<Resolution> {
    keys.iter()
        .map(|key| Resolution::new(key, ResolutionState::Unavailable))
        .collect()
}

#[cfg(test)]
mod tests;
