//! What a chat knows about its running turn, for failure reports.

use std::sync::Arc;
use std::time::Instant;

use serde_json::Value;

use crate::ChatEvent;

/// What the runtime knows about the turn that is running, or about the chat
/// when no turn runs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TurnContext {
    /// When the turn was sent; `None` while no turn runs.
    pub sent: Option<Instant>,
    /// Context references and images sent with the turn, steering included.
    pub attachments: usize,
    /// Whether any reply or reasoning text arrived.
    pub delta_received: bool,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// The texts the turn sent: the message and each context item. They are
    /// kept only to remove them from what a failure shows or logs.
    pub submitted: Arc<Vec<String>>,
}

impl TurnContext {
    /// A new turn. The chat's model and effort carry over.
    pub fn begin(&mut self, attachments: usize, submitted: Vec<String>) {
        self.sent = Some(Instant::now());
        self.attachments = attachments;
        self.delta_received = false;
        self.submitted = Arc::new(submitted);
    }

    /// A steering message joins the running turn with its attachments.
    pub fn steer(&mut self, attachments: usize, submitted: Vec<String>) {
        self.attachments += attachments;
        Arc::make_mut(&mut self.submitted).extend(submitted);
    }

    /// The runtime went on with a turn the watchdog had ended: the same
    /// message, timed from now.
    pub fn resume(&mut self) {
        self.sent = Some(Instant::now());
    }

    /// The turn ended. The model and effort stay for the next one.
    pub fn finish(&mut self) {
        self.sent = None;
    }

    /// Takes what one SDK event says about the turn: whether text arrived,
    /// and the model and effort Copilot used or switched to.
    pub fn observe(&mut self, event: &Value) {
        let data = &event["data"];
        match event["type"].as_str().unwrap_or_default() {
            "assistant.message_delta" | "assistant.reasoning_delta" => self.delta_received = true,
            "assistant.message" => {
                self.delta_received |= data["content"]
                    .as_str()
                    .is_some_and(|text| !text.is_empty())
            }
            "assistant.usage" => {
                self.select(data["model"].as_str(), data["reasoningEffort"].as_str())
            }
            "session.model_change" => {
                self.select(data["newModel"].as_str(), data["reasoningEffort"].as_str())
            }
            _ => {}
        }
    }

    /// Keeps the model selection the runtime confirmed for the chat.
    pub fn confirm(&mut self, event: Option<&ChatEvent>) {
        if let Some(ChatEvent::SelectedModel { model, effort }) = event {
            self.model = Some(model.clone());
            self.effort = effort.clone();
        }
    }

    /// Keeps a model the runtime reported, with its effort.
    pub fn select(&mut self, model: Option<&str>, effort: Option<&str>) {
        let Some(model) = model.filter(|model| !model.is_empty()) else {
            return;
        };
        self.model = Some(model.to_owned());
        self.effort = effort.map(str::to_owned);
    }
}
