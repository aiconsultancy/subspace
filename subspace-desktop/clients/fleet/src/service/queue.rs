//! The one ordered path for each fleet's token: every read, save and clear
//! of a fleet's token runs after every one queued before it for that
//! fleet, and its result is applied before the next one starts. Each fleet
//! has its own path, so one waiting on the keychain never holds up
//! another. The keychain and the service so
//! change in the order the operations were asked for, whatever the threads
//! do: a save queued before a clear writes before the clear deletes, and a
//! read queued before either reads first.
//!
//! Each operation carries the fleet's token generation from when it was
//! queued. A save, a clear or the fleet's removal starts a new generation,
//! so a read queued before one of them lands in an older generation and is
//! dropped. A save that fails while still current reads the store again,
//! so the service shows what the keychain holds rather than waiting for
//! ever. A clear that fails leaves the token out of use
//! ([`TokenLoad::ClearFailed`]): a cleared token is never used again until
//! the person clears it again or saves a new one.

use std::sync::Arc;

use gpui_kit::{AppContext as _, Context, Task};
use subspace_framework::core::Secret;

use super::{FleetApi, TokenError};
use crate::FleetAddress;
use crate::directory::TokenLoad;
use crate::tokens::{SavedToken, StoreError, TokenStore};

/// What one queued operation does.
pub(super) enum Op {
    Read,
    /// Saves the token, already checked, for the address.
    Save(FleetAddress, Secret),
    Clear,
}

/// What an operation found.
enum Outcome {
    Read(TokenLoad),
    Saved(Result<SavedToken, StoreError>),
    Cleared(Result<(), StoreError>),
}

/// The answer when the service went before the operation could land.
const GONE: TokenError = TokenError::Store(StoreError::Unavailable);

impl FleetApi {
    /// Queues `op` for `fleet` behind every store operation already queued,
    /// in the fleet's current generation. The task says how it went.
    pub(super) fn enqueue(
        &mut self,
        fleet: &str,
        op: Op,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), TokenError>> {
        let generation = self.generation_of(fleet);
        let previous = self.reads.tails.remove(fleet);
        let store = Arc::clone(&self.store);
        let id = fleet.to_owned();
        let (answer, answered) = tokio::sync::oneshot::channel();
        let tail = cx.spawn(async move |this, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            let account = id.clone();
            let outcome = cx
                .background_spawn(async move { run(&store, &account, op) })
                .await;
            let result = this
                .update(cx, |this, cx| this.land(&id, generation, outcome, cx))
                .unwrap_or(Err(GONE));
            _ = answer.send(result);
        });
        self.reads.tails.insert(fleet.to_owned(), tail);
        cx.background_spawn(async move { answered.await.unwrap_or(Err(GONE)) })
    }

    /// Applies what an operation in `generation` found, if nothing has
    /// superseded it since it was queued. A failed save or clear that is
    /// still current reads the store again.
    fn land(
        &mut self,
        fleet: &str,
        generation: u64,
        outcome: Outcome,
        cx: &mut Context<Self>,
    ) -> Result<(), TokenError> {
        let current = generation == self.generation_of(fleet);
        if matches!(outcome, Outcome::Read(_)) {
            self.reads.rereading.remove(fleet);
        }
        let (token, result) = match outcome {
            Outcome::Read(token) => (Some(token), Ok(())),
            Outcome::Saved(Ok(saved)) => (Some(TokenLoad::Saved(saved)), Ok(())),
            Outcome::Cleared(Ok(())) => (None, Ok(())),
            Outcome::Cleared(Err(error)) => {
                (Some(TokenLoad::ClearFailed), Err(TokenError::Store(error)))
            }
            Outcome::Saved(Err(error)) => (None, Err(TokenError::Store(error))),
        };
        match (current, token, &result) {
            (true, Some(token), _) => self.set_token(fleet, token, cx),
            (true, None, Err(_)) => self.load_token(fleet.to_owned(), cx),
            _ => {}
        }
        result
    }
}

/// Runs `op` on `store`. Blocks on the keychain.
fn run(store: &TokenStore, fleet: &str, op: Op) -> Outcome {
    match op {
        Op::Read => Outcome::Read(TokenLoad::read(store, fleet)),
        Op::Save(address, token) => Outcome::Saved(
            store
                .save(fleet, &address, &token)
                .map(|()| SavedToken::new(&address, token)),
        ),
        Op::Clear => Outcome::Cleared(store.clear(fleet)),
    }
}
