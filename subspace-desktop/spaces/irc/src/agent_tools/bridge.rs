//! Carries tool calls to the hub, which answers them on the UI thread in its
//! next drain, so a tool reads and sends exactly what the board shows.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use serde_json::Value;
use subspace_client_irc::Command;

use super::{read, write};
use crate::state::IrcState;

/// How long a call waits for the hub, which drains every 50 ms.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(10);

const NOT_RUNNING: &str =
    "The Channels Space is not running in this Subspace app. Turn it on under Settings > Channels.";

const NO_ANSWER: &str = "The Channels Space did not answer in time. A write may still run: read the buffer before trying again.";

/// The running hub's request queue; `None` while the board is off.
static HUB: Mutex<Option<Sender<Request>>> = Mutex::new(None);

pub(super) type Reply<T> = Sender<Result<T, String>>;

/// One tool call for the hub, with the channel its answer goes back on.
pub(crate) enum Request {
    Read(read::Request, Reply<Value>),
    /// Checks a write batch before the approval; answers one label each.
    Check(Vec<write::Write>, Reply<Vec<String>>),
    Write(write::Write, Reply<Value>),
}

/// Where a tool sends its requests.
#[derive(Clone)]
pub(super) enum Link {
    /// The hub of this process, while the board runs.
    Process,
    #[cfg(test)]
    Fixed(Sender<Request>),
}

impl Link {
    fn sender(&self) -> Option<Sender<Request>> {
        match self {
            Self::Process => HUB.lock().unwrap_or_else(PoisonError::into_inner).clone(),
            #[cfg(test)]
            Self::Fixed(sender) => Some(sender.clone()),
        }
    }

    /// Sends the request `build` makes around its reply channel and waits
    /// for the answer. Tools call this off the UI thread.
    pub(super) fn ask<T>(&self, build: impl FnOnce(Reply<T>) -> Request) -> Result<T, String> {
        let sender = self.sender().ok_or(NOT_RUNNING)?;
        let (reply, answer) = mpsc::channel();
        sender
            .send(build(reply))
            .map_err(|_| NOT_RUNNING.to_owned())?;
        answer
            .recv_timeout(ANSWER_TIMEOUT)
            .map_err(|_| NO_ANSWER.to_owned())?
    }
}

/// Takes this process's tool calls for a starting hub.
pub(crate) fn open() -> Receiver<Request> {
    let (sender, receiver) = mpsc::channel();
    *HUB.lock().unwrap_or_else(PoisonError::into_inner) = Some(sender);
    receiver
}

/// Refuses further tool calls: the board stopped.
pub(crate) fn close() {
    *HUB.lock().unwrap_or_else(PoisonError::into_inner) = None;
}

/// Answers `request` from the hub's state and returns the commands the hub
/// sends. `agents` says which nicks are fleet agents, which no message
/// reaches. A caller that stopped waiting no longer receives the answer.
pub(crate) fn serve(
    state: &mut IrcState,
    request: Request,
    agents: &write::Agents,
) -> Vec<Command> {
    match request {
        Request::Read(request, reply) => answer(reply, read::answer(state, request)),
        Request::Check(operations, reply) => answer(
            reply,
            (write::check_all(state, &operations, agents), Vec::new()),
        ),
        Request::Write(operation, reply) => answer(reply, write::apply(state, &operation, agents)),
    }
}

/// Refuses `request` with `message`, as while Channels is closed.
pub(crate) fn refuse(request: Request, message: &str) {
    let message = message.to_owned();
    match request {
        Request::Read(_, reply) => {
            let _ = reply.send(Err(message));
        }
        Request::Check(_, reply) => {
            let _ = reply.send(Err(message));
        }
        Request::Write(_, reply) => {
            let _ = reply.send(Err(message));
        }
    }
}

fn answer<T>(
    reply: Reply<T>,
    (result, commands): (Result<T, String>, Vec<Command>),
) -> Vec<Command> {
    let _ = reply.send(result);
    commands
}
