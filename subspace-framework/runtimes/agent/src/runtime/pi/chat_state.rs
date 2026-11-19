//! What one Pi chat knows about its models, its startup and its turn.
use super::{Arc, AtomicBool, ModelChoice, Ordering, Value};

pub(super) const NO_MODEL: &str = "Pi has no configured model. Configure Pi authentication and a model outside Subspace, then restart Subspace.";
const CHECKING_MODELS: &str = "Pi is still checking its configured models. Try again shortly.";
/// The three startup answers, one bit each.
const STARTUP_RESPONSES: u8 = 0b111;

/// What Pi reported about its configured models.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Models {
    Unknown,
    None,
    Available,
}

impl Models {
    pub(super) fn listed(choices: &[ModelChoice]) -> Self {
        if choices.is_empty() {
            Self::None
        } else {
            Self::Available
        }
    }

    /// A message needs a configured model.
    pub(super) fn require(self) -> Result<(), String> {
        match self {
            Self::Unknown => Err(CHECKING_MODELS.into()),
            Self::None => Err(NO_MODEL.into()),
            Self::Available => Ok(()),
        }
    }
}

/// Which of the startup answers arrived, until the chat is reported ready.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Startup {
    Pending(u8),
    Ready,
}

impl Startup {
    pub(super) fn is_pending(self) -> bool {
        matches!(self, Self::Pending(_))
    }

    /// Every startup answer arrived, but the chat is not reported ready yet.
    pub(super) fn is_complete(self) -> bool {
        self == Self::Pending(STARTUP_RESPONSES)
    }

    pub(super) fn received(self, response: u8) -> Self {
        match self {
            Self::Pending(received) => Self::Pending(received | response),
            Self::Ready => Self::Ready,
        }
    }
}

/// Whether Pi runs a turn for the chat.
#[derive(Debug)]
pub(super) enum Turn {
    Idle,
    Running {
        /// The request id of the prompt that started the turn.
        prompt: String,
        /// A message sent during the turn steered it.
        steered: bool,
        /// Set by Stop, and read by the turn's tool calls. A steering message
        /// joins the turn, so it keeps the cancellation.
        cancelled: Arc<AtomicBool>,
    },
}

impl Turn {
    pub(super) fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    pub(super) fn steered(&self) -> bool {
        matches!(self, Self::Running { steered: true, .. })
    }

    /// Pi took the prompt `id`: it starts a turn, or steers the running one.
    pub(super) fn sent(&mut self, id: String) {
        match self {
            Self::Running { steered, .. } => *steered = true,
            Self::Idle => {
                *self = Self::Running {
                    prompt: id,
                    steered: false,
                    cancelled: Arc::default(),
                }
            }
        }
    }

    /// Pi answered a prompt without a run: it refused it, or handled it
    /// without the agent. Returns whether that ended the turn.
    pub(super) fn ran_nothing(&mut self, answer: &Value) -> bool {
        let refused = answer["success"].as_bool() != Some(true);
        let handled = answer["data"]["disposition"].as_str() == Some("handled");
        let prompt = answer["command"].as_str() == Some("prompt");
        prompt && (refused || handled) && self.unstarted(answer["id"].as_str())
    }

    /// The prompt `id` started no run. When it was the prompt that started
    /// the turn, the turn ends; a steering message leaves it running.
    fn unstarted(&mut self, id: Option<&str>) -> bool {
        let started = matches!(self, Self::Running { prompt, .. } if Some(prompt.as_str()) == id);
        if started {
            *self = Self::Idle;
        }
        started
    }

    pub(super) fn cancel(&self) {
        if let Self::Running { cancelled, .. } = self {
            cancelled.store(true, Ordering::Release);
        }
    }

    /// What a tool call of the turn watches for Stop. Outside a turn
    /// nothing cancels it.
    pub(super) fn cancellation(&self) -> Arc<AtomicBool> {
        match self {
            Self::Running { cancelled, .. } => cancelled.clone(),
            Self::Idle => Arc::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::{Models, Startup, Turn};

    #[test]
    fn a_message_waits_for_listed_models() {
        assert!(Models::Unknown.require().is_err());
        assert_eq!(Models::listed(&[]), Models::None);
        assert!(Models::None.require().is_err());
        assert!(Models::Available.require().is_ok());
    }

    #[test]
    fn startup_completes_once_every_answer_arrived() {
        let mut startup = Startup::Pending(0);
        for response in [0b001, 0b100] {
            startup = startup.received(response);
        }
        assert!(startup.is_pending() && !startup.is_complete());
        startup = startup.received(0b010);
        assert!(startup.is_complete());
        assert!(!Startup::Ready.received(0b001).is_pending());
    }

    fn answer(id: &str, success: bool, disposition: &str) -> serde_json::Value {
        serde_json::json!({
            "type": "response", "id": id, "command": "prompt", "success": success,
            "data": {"disposition": disposition}
        })
    }

    #[test]
    fn a_rejected_steer_keeps_the_turn_and_the_first_prompt_ends_it() {
        let mut turn = Turn::Idle;
        turn.sent("prompt-1".into());
        assert!(!turn.steered());
        assert!(!turn.ran_nothing(&answer("prompt-1", true, "started")));
        turn.sent("prompt-2".into());
        assert!(turn.steered());
        let steer = answer("prompt-2", false, "");
        assert!(!turn.ran_nothing(&steer), "a refused steer ends nothing");
        assert!(turn.is_running());
        assert!(turn.ran_nothing(&answer("prompt-1", false, "")));
        assert!(!turn.is_running() && !turn.steered());
    }

    #[test]
    fn a_handled_first_prompt_ends_the_turn() {
        let mut turn = Turn::Idle;
        turn.sent("prompt-1".into());
        assert!(turn.ran_nothing(&answer("prompt-1", true, "handled")));
        assert!(!turn.is_running());
    }

    #[test]
    fn stop_then_a_steer_keeps_the_turn_cancelled() {
        let mut turn = Turn::Idle;
        turn.sent("prompt-1".into());
        let watched = turn.cancellation();
        turn.cancel();
        turn.sent("prompt-2".into());
        assert!(watched.load(Ordering::Acquire));
        assert!(
            turn.cancellation().load(Ordering::Acquire),
            "new tool calls too"
        );
        turn = Turn::Idle;
        turn.sent("prompt-3".into());
        assert!(
            !turn.cancellation().load(Ordering::Acquire),
            "a new turn is not"
        );
    }
}
