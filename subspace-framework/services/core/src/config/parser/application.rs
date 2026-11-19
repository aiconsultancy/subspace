use serde_json::{Map, Value};

use super::Parser;
use crate::config::{ChatEngine, show};

impl Parser<'_> {
    pub(super) fn board_order(&mut self, value: Option<&Value>) -> Vec<String> {
        let Some(value) = value else {
            return Vec::new();
        };
        let Some(values) = value.as_array() else {
            self.warn("boardOrder", "expected an array; using the default order");
            return Vec::new();
        };
        values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    }

    pub(super) fn auto_refresh(&mut self, value: Option<&Value>) -> bool {
        match value {
            None | Some(Value::Null) => true,
            Some(Value::Bool(enabled)) => *enabled,
            Some(value) => {
                self.warn(
                    "autoRefresh",
                    format!("expected true or false, got {}; using true", show(value)),
                );
                true
            }
        }
    }

    pub(super) fn chat_engine(&mut self, section: Option<&Map<String, Value>>) -> ChatEngine {
        let Some(value) = section.and_then(|chat| chat.get("engine")) else {
            return ChatEngine::default();
        };
        match value {
            Value::Null => ChatEngine::default(),
            Value::String(value) if value == "off" => ChatEngine::Off,
            Value::String(value) if value == "copilot" => ChatEngine::Copilot,
            Value::String(value) if value == "pi" => ChatEngine::Pi,
            Value::String(value) if value == "acp" => ChatEngine::Acp,
            value => {
                self.warn(
                    "chat.engine",
                    format!(
                        "expected \"off\", \"copilot\", \"pi\" or \"acp\", got {}; using the default",
                        show(value)
                    ),
                );
                ChatEngine::default()
            }
        }
    }
}
