use super::{
    Hub, IrcSnapshot, ReadPriority, ReferenceCard, ResolvedReference, Shared, lock,
    worker::{Job, Task},
};
use crate::resolution_cache::{ResolutionLookup, ResolutionOutcome};
use std::time::Instant;

impl Hub {
    // qual:allow(dry, boilerplate) reason: "The independent contract projection owns its minimal reference data and safe errors."
    pub fn irc_snapshot(&self) -> IrcSnapshot {
        let state = lock(&self.0.state);
        let live = state.references_live();
        let mut cards: Vec<_> = if live {
            state
                .references
                .cards(Instant::now())
                .map(|(id, card)| ResolvedReference::new(id, card))
                .collect()
        } else {
            vec![]
        };
        cards.sort_by(|left, right| left.identifier().cmp(right.identifier()));
        IrcSnapshot {
            version: state.snapshot.version,
            profile_id: state.snapshot.profile_id.clone(),
            profile_name: state.snapshot.profile_name.clone(),
            api_origin: state.api_origin.clone(),
            project_keys: if live {
                state
                    .snapshot
                    .projects
                    .iter()
                    .map(|project| project.key.clone())
                    .collect()
            } else {
                vec![]
            },
            cards,
            errors: state.reference_errors.entries().clone(),
            account_error: match &state.snapshot.status {
                super::HubStatus::Error(
                    error @ (super::ReadError::SignInNeeded | super::ReadError::Forbidden),
                ) => Some(error.clone()),
                _ => None,
            },
        }
    }
    pub fn reference_version(&self) -> u64 {
        let mut state = lock(&self.0.state);
        state.reference_version(Instant::now())
    }
    pub fn resolve_reference(&self, identifier: &str, profile_id: &str) {
        let mut state = lock(&self.0.state);
        if profile_id != crate::PROFILE_ID || !state.references_live() {
            return;
        }
        if let ResolutionLookup::Start(read) = state.references.request(identifier, Instant::now())
        {
            state.reference_errors.remove(identifier);
            self.0
                .enqueue(&mut state, Task::Reference(read), ReadPriority::Reference);
        }
    }
    /// Reads `identifier` again because it changed, such as after a write
    /// from Channels: the card held now stays until the new read replaces
    /// it, and the reference version moves when it lands.
    pub fn refresh_reference(&self, identifier: &str, profile_id: &str) {
        let mut state = lock(&self.0.state);
        if profile_id != crate::PROFILE_ID || !state.references_live() {
            return;
        }
        let read = state.references.refresh(identifier, Instant::now());
        state.reference_errors.remove(identifier);
        self.0
            .enqueue(&mut state, Task::Reference(read), ReadPriority::Reference);
    }
}

pub(super) fn resolve(shared: &Shared, job: Job) {
    let Task::Reference(read) = job.task else {
        return;
    };
    let result = shared.client.issue(read.identifier());
    let mut state = lock(&shared.state);
    if !state.current(job.generation) || !state.references.is_pending(&read) {
        return;
    }
    let outcome = match result {
        Ok(detail) => ResolutionOutcome::Available(ReferenceCard::from_detail(detail)),
        Err(crate::Error::NotFound) => ResolutionOutcome::Unavailable,
        Err(error) => {
            let denied = matches!(error, crate::Error::SignInNeeded | crate::Error::Forbidden);
            state
                .reference_errors
                .insert(read.identifier(), error.into());
            if denied {
                ResolutionOutcome::Denied
            } else {
                ResolutionOutcome::Error
            }
        }
    };
    state.references.finish(read, outcome, Instant::now());
    state.changed();
}
