//! Board-to-board communication through contracts. Boards never reference
//! each other: a board declares, in [`Dashboard::contracts`](crate::Dashboard::contracts),
//! which messages of which contracts (`subspace_contracts`) it publishes,
//! subscribes to, requests and answers, and the broker routes them between
//! the running boards.
//!
//! - [`publish`] queues an event. Delivery waits until the publisher's update
//!   ends (`App::defer`), runs on the UI thread, keeps each publisher's order
//!   and reaches subscribers in product board order. It is at most once, in
//!   process, with no replay. A chain of events published from handlers
//!   deeper than eight is dropped, so a cycle cannot loop.
//! - [`subscribe`] attaches a receiver entity, such as a board's process-owned
//!   hub started in `start_background`, or its view. The broker keeps only a
//!   weak handle: a dropped receiver is skipped and forgotten, and a stopped
//!   or disabled board receives nothing.
//! - [`request`] asks the one running board that answers the request, at
//!   once, from the state it holds. With no such board, or when its responder
//!   is gone, the answer is [`ContractError::Unavailable`], a normal outcome.
//! - [`respond`] attaches the answering entity, again by weak handle. A
//!   responder must not block: it answers from memory and starts slow work
//!   itself, announcing the result with an event.
//!
//! Undeclared messages are refused with [`ContractError::Undeclared`] and
//! logged with the board and message names only, never payloads.

mod broker;
mod declaration;
pub mod github;

use std::fmt;
use std::rc::Rc;

use gpui_kit::{App, Context, Entity};
use subspace_contracts::{Event, Request};

use broker::{Answer, Broker, Delivery, Endpoint, Receive, Receivers, Responders};
pub use declaration::{BoardContracts, Declaration, Role, validate};
pub use subspace_contracts::{Contract, MessageKey, MessageKind};

const LOG_TARGET: &str = "contracts";

/// Why the broker did not route a message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractError {
    /// The board is not running or did not declare this role for the message.
    Undeclared {
        board: &'static str,
        role: Role,
        key: MessageKey,
    },
    /// No running board answers the request, or its responder is gone or
    /// already answering.
    Unavailable { key: MessageKey },
    /// The event was published from a chain of handlers deeper than allowed.
    TooDeep {
        board: &'static str,
        key: MessageKey,
    },
}

impl fmt::Display for ContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Undeclared { board, role, key } => write!(
                formatter,
                "board {board} has not declared that it {} {}",
                role.as_str(),
                key.label()
            ),
            Self::Unavailable { key } => {
                write!(formatter, "no running board answers {}", key.label())
            }
            Self::TooDeep { board, key } => write!(
                formatter,
                "board {board} published {} from a chain of more than {} events",
                key.label(),
                broker::MAX_DEPTH
            ),
        }
    }
}

impl std::error::Error for ContractError {}

/// Sets the running boards and their declarations, in product order. The
/// host calls this whenever the running set changes; receivers stay attached.
pub fn install(boards: Vec<(&'static str, BoardContracts)>, cx: &mut App) {
    cx.default_global::<Broker>().install(boards);
}

/// Queues `event` from board `publisher` for every subscribed running board.
pub fn publish<E: Event>(
    publisher: &'static str,
    event: E,
    cx: &mut App,
) -> Result<(), ContractError> {
    let deliver: Delivery = Box::new(move |boards, cx| deliver(&event, boards, cx));
    let queued = cx
        .default_global::<Broker>()
        .enqueue(publisher, MessageKey::event::<E>(), deliver)
        .map_err(logged)?;
    if queued {
        cx.defer(flush);
    }
    Ok(())
}

/// Delivers board `board`'s events of type `E` to `receiver`, while it lives.
pub fn subscribe<E: Event, T: 'static>(
    board: &'static str,
    receiver: &Entity<T>,
    handler: impl Fn(&mut T, &E, &mut Context<T>) + 'static,
    cx: &mut App,
) -> Result<(), ContractError> {
    let declared =
        cx.default_global::<Broker>()
            .check(board, Role::Subscribes, MessageKey::event::<E>());
    declared.map_err(logged)?;
    let weak = receiver.downgrade();
    let call: Receive<E> = Rc::new(move |event, cx| {
        weak.update(cx, |this, cx| handler(this, event, cx)).ok();
    });
    let endpoint = Endpoint {
        board,
        entity: receiver.downgrade().into(),
        call,
    };
    cx.default_global::<Receivers<E>>()
        .0
        .attach(endpoint, |old, new| {
            old.board == new.board && old.entity.entity_id() == new.entity.entity_id()
        });
    Ok(())
}

/// Answers board `board`'s requests of type `R` with `responder`, while it lives.
pub fn respond<R: Request, T: 'static>(
    board: &'static str,
    responder: &Entity<T>,
    handler: impl Fn(&mut T, &R, &mut Context<T>) -> R::Response + 'static,
    cx: &mut App,
) -> Result<(), ContractError> {
    let declared =
        cx.default_global::<Broker>()
            .check(board, Role::Responds, MessageKey::request::<R>());
    declared.map_err(logged)?;
    let weak = responder.downgrade();
    let call: Answer<R> =
        Rc::new(move |request, cx| weak.update(cx, |this, cx| handler(this, request, cx)).ok());
    let endpoint = Endpoint {
        board,
        entity: responder.downgrade().into(),
        call,
    };
    cx.default_global::<Responders<R>>()
        .0
        .attach(endpoint, |old, new| old.board == new.board);
    Ok(())
}

/// Asks the running board that answers `R`, from board `requester`.
pub fn request<R: Request>(
    requester: &'static str,
    request: &R,
    cx: &mut App,
) -> Result<R::Response, ContractError> {
    let key = MessageKey::request::<R>();
    let unavailable = ContractError::Unavailable { key };
    let boards = cx
        .default_global::<Broker>()
        .responders(requester, key)
        .map_err(logged)?;
    let calls = cx.default_global::<Responders<R>>().0.calls(&boards);
    let (board, answer) = calls.into_iter().next().ok_or(unavailable)?;
    cx.default_global::<Broker>().begin_answer(board);
    let response = answer(request, cx);
    cx.default_global::<Broker>().end_answer(board);
    response.ok_or(unavailable)
}

fn deliver<E: Event>(event: &E, boards: &[&'static str], cx: &mut App) {
    let calls = cx.default_global::<Receivers<E>>().0.calls(boards);
    for (_, call) in calls {
        call(event, cx);
    }
}

fn flush(cx: &mut App) {
    while let Some((deliver, boards)) = cx.default_global::<Broker>().next_delivery() {
        deliver(&boards, cx);
    }
}

/// Logs a refusal; an unavailable request is a normal outcome and is not logged.
fn logged(error: ContractError) -> ContractError {
    if !matches!(error, ContractError::Unavailable { .. }) {
        subspace_core::log::Logger::new(LOG_TARGET).warn(error.to_string());
    }
    error
}

/// The running board that answers a declared request, for shell navigation.
pub fn navigation_target(
    requester: &'static str,
    key: MessageKey,
    cx: &App,
) -> Option<&'static str> {
    cx.try_global::<Broker>()?
        .responders(requester, key)
        .ok()?
        .into_iter()
        .next()
}

#[cfg(test)]
mod tests;
