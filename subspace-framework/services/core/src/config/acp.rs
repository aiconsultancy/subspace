//! `chat.acp`: the Agent Client Protocol agent that the ACP chat engine runs.
//! The runtime owns each preset's command; this file only names the choice.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// An ACP agent preset. `Custom` runs `chat.acp.command`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AcpAgent {
    #[default]
    OpenCode,
    Claude,
    Codex,
    Gemini,
    Custom,
}

impl AcpAgent {
    /// Every preset, in the order Settings lists them.
    pub const ALL: [Self; 5] = [
        Self::OpenCode,
        Self::Claude,
        Self::Codex,
        Self::Gemini,
        Self::Custom,
    ];

    /// The `chat.acp.agent` value.
    pub fn id(self) -> &'static str {
        self.names().0
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|agent| agent.id() == id)
    }

    /// The agent's own product name.
    pub fn label(self) -> &'static str {
        self.names().1
    }

    /// The short name chat shows for the engine, such as "Codex ready".
    pub fn engine_label(self) -> &'static str {
        self.names().2
    }

    /// The config id, the product name and the engine name.
    fn names(self) -> (&'static str, &'static str, &'static str) {
        match self {
            AcpAgent::OpenCode => ("opencode", "OpenCode", "OpenCode"),
            AcpAgent::Claude => ("claude", "Claude Code", "Claude"),
            AcpAgent::Codex => ("codex", "Codex", "Codex"),
            AcpAgent::Gemini => ("gemini", "Gemini CLI", "Gemini"),
            AcpAgent::Custom => ("custom", "Custom agent", "ACP agent"),
        }
    }
}

/// The selected agent, and an optional command that replaces its preset.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AcpConfig {
    pub agent: AcpAgent,
    /// The executable to run instead of the preset's. `custom` requires it.
    pub command: Option<String>,
    /// The arguments to pass instead of the preset's.
    pub args: Option<Vec<String>>,
    /// Extra environment variables for the agent process.
    pub env: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::AcpAgent;

    #[test]
    fn every_agent_round_trips_through_its_config_id() {
        for agent in AcpAgent::ALL {
            assert_eq!(AcpAgent::from_id(agent.id()), Some(agent));
            assert_eq!(
                serde_json::to_value(agent).unwrap(),
                serde_json::Value::String(agent.id().into())
            );
        }
        assert_eq!(AcpAgent::from_id("cursor"), None);
        assert_eq!(AcpAgent::default(), AcpAgent::OpenCode);
        assert_eq!(AcpAgent::OpenCode.engine_label(), "OpenCode");
        assert_eq!(AcpAgent::Claude.engine_label(), "Claude");
        assert_eq!(AcpAgent::Gemini.label(), "Gemini CLI");
        assert_eq!(AcpAgent::Gemini.engine_label(), "Gemini");
    }
}
