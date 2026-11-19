//! The broker's state: the running boards' declarations and the queue of
//! events waiting for delivery, plus one typed global per message type for
//! its weak receivers or responders, so nothing is type-erased and cast back.

use std::collections::VecDeque;
use std::rc::Rc;

use gpui_kit::{AnyWeakEntity, App, Global};
use subspace_contracts::{MessageKey, Request};

use super::{BoardContracts, ContractError, Role};

/// The longest chain of events published from event handlers.
pub(super) const MAX_DEPTH: usize = 8;

/// Delivers one queued event to the receivers of the given boards.
pub(super) type Delivery = Box<dyn FnOnce(&[&'static str], &mut App)>;
pub(super) type Receive<E> = Rc<dyn Fn(&E, &mut App)>;
pub(super) type Answer<R> = Rc<dyn Fn(&R, &mut App) -> Option<<R as Request>::Response>>;

struct Pending {
    publisher: &'static str,
    key: MessageKey,
    depth: usize,
    deliver: Delivery,
}

#[derive(Default)]
pub(super) struct Broker {
    /// The running boards, in product order.
    boards: Vec<(&'static str, BoardContracts)>,
    queue: VecDeque<Pending>,
    /// A deferred flush is scheduled or running.
    flushing: bool,
    /// The depth of the event being delivered.
    depth: Option<usize>,
    /// Boards answering a request now, so a cycle of requests cannot re-enter.
    answering: Vec<&'static str>,
}

impl Global for Broker {}

impl Broker {
    pub(super) fn install(&mut self, boards: Vec<(&'static str, BoardContracts)>) {
        self.boards = boards;
    }

    /// Whether running board `board` declares `role` for `key`.
    pub(super) fn check(
        &self,
        board: &'static str,
        role: Role,
        key: MessageKey,
    ) -> Result<(), ContractError> {
        let declared = self
            .boards
            .iter()
            .any(|(id, contracts)| *id == board && contracts.declares(role, key));
        match declared {
            true => Ok(()),
            false => Err(ContractError::Undeclared { board, role, key }),
        }
    }

    /// Queues an event. `Ok(true)` asks the caller to schedule a flush.
    pub(super) fn enqueue(
        &mut self,
        publisher: &'static str,
        key: MessageKey,
        deliver: Delivery,
    ) -> Result<bool, ContractError> {
        self.check(publisher, Role::Publishes, key)?;
        let depth = self.depth.map_or(0, |depth| depth + 1);
        if depth > MAX_DEPTH {
            return Err(ContractError::TooDeep {
                board: publisher,
                key,
            });
        }
        self.queue.push_back(Pending {
            publisher,
            key,
            depth,
            deliver,
        });
        Ok(!std::mem::replace(&mut self.flushing, true))
    }

    /// The next queued event with the running boards it reaches, in product
    /// order, leaving out its publisher. `None` ends the flush.
    pub(super) fn next_delivery(&mut self) -> Option<(Delivery, Vec<&'static str>)> {
        let Some(pending) = self.queue.pop_front() else {
            self.flushing = false;
            self.depth = None;
            return None;
        };
        self.depth = Some(pending.depth);
        let boards = self
            .boards
            .iter()
            .filter(|(id, contracts)| {
                *id != pending.publisher && contracts.declares(Role::Subscribes, pending.key)
            })
            .map(|(id, _)| *id)
            .collect();
        Some((pending.deliver, boards))
    }

    /// The running boards, in product order, that may answer `key` for
    /// `requester` now: not the requester and not already answering.
    pub(super) fn responders(
        &self,
        requester: &'static str,
        key: MessageKey,
    ) -> Result<Vec<&'static str>, ContractError> {
        self.check(requester, Role::Requests, key)?;
        Ok(self
            .boards
            .iter()
            .filter(|(id, contracts)| {
                *id != requester
                    && !self.answering.contains(id)
                    && contracts.declares(Role::Responds, key)
            })
            .map(|(id, _)| *id)
            .collect())
    }

    pub(super) fn begin_answer(&mut self, board: &'static str) {
        self.answering.push(board);
    }

    pub(super) fn end_answer(&mut self, board: &'static str) {
        if let Some(index) = self.answering.iter().rposition(|id| *id == board) {
            self.answering.remove(index);
        }
    }
}

/// A receiver or responder: an entity held only by a weak handle.
pub(super) struct Endpoint<F> {
    pub(super) board: &'static str,
    pub(super) entity: AnyWeakEntity,
    pub(super) call: F,
}

pub(super) struct Endpoints<F>(Vec<Endpoint<F>>);

impl<F> Default for Endpoints<F> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<F: Clone> Endpoints<F> {
    /// Adds `endpoint` after forgetting dropped ones and those it replaces.
    pub(super) fn attach(
        &mut self,
        endpoint: Endpoint<F>,
        replaces: fn(&Endpoint<F>, &Endpoint<F>) -> bool,
    ) {
        self.0
            .retain(|old| old.entity.is_upgradable() && !replaces(old, &endpoint));
        self.0.push(endpoint);
    }

    /// The live endpoints of `boards`, in that order, then attach order.
    pub(super) fn calls(&mut self, boards: &[&'static str]) -> Vec<(&'static str, F)> {
        self.0.retain(|endpoint| endpoint.entity.is_upgradable());
        boards
            .iter()
            .flat_map(|board| {
                self.0
                    .iter()
                    .filter(move |endpoint| endpoint.board == *board)
            })
            .map(|endpoint| (endpoint.board, endpoint.call.clone()))
            .collect()
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.0.len()
    }
}

/// The receivers of event type `E`.
pub(super) struct Receivers<E: 'static>(pub(super) Endpoints<Receive<E>>);

impl<E: 'static> Default for Receivers<E> {
    fn default() -> Self {
        Self(Endpoints::default())
    }
}

impl<E: 'static> Global for Receivers<E> {}

/// The responders to request type `R`.
pub(super) struct Responders<R: Request>(pub(super) Endpoints<Answer<R>>);

impl<R: Request> Default for Responders<R> {
    fn default() -> Self {
        Self(Endpoints::default())
    }
}

impl<R: Request> Global for Responders<R> {}
