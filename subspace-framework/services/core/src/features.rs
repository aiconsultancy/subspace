//! Registered feature policy shared by configuration, settings and runtimes.

/// A registered feature's lifecycle stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeatureStage {
    Experimental,
    Stable,
    Removed,
}

/// Static metadata and policy for one registered feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeatureSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub stage: FeatureStage,
    pub default_enabled: bool,
    pub toggleable: bool,
}

/// Pi chat is opt-in while its adapter is limited to Pi 1.0.4 and does not
/// provide Copilot's sandbox, native host tools or Copilot-only controls.
pub const PI_CHAT: FeatureSpec = FeatureSpec {
    id: "app.pi-chat",
    title: "Pi chat",
    description: "Alternative local chat limited to Pi 1.0.4, without Copilot's sandbox, native host tools, automatic resource discovery, or Copilot-only controls.",
    stage: FeatureStage::Experimental,
    default_enabled: false,
    toggleable: true,
};

/// ACP chat is opt-in while it runs external agents whose own tools, sandbox
/// and permission rules Subspace does not control.
pub const ACP_CHAT: FeatureSpec = FeatureSpec {
    id: "app.acp-chat",
    title: "ACP agent chat",
    description: "Alternative local chat with an Agent Client Protocol agent such as OpenCode, Claude Code, Codex or Gemini CLI. The agent keeps its own tools, sandbox and sign-in; Subspace shows its permission requests and serves board tools to it over MCP.",
    stage: FeatureStage::Experimental,
    default_enabled: false,
    toggleable: true,
};

/// A bounded app or board feature id made from nonempty ASCII segments.
pub fn valid_id(id: &str) -> bool {
    const MAX_ID_BYTES: usize = 128;
    if id.is_empty() || id.len() > MAX_ID_BYTES || !id.is_ascii() {
        return false;
    }
    let segments = id.split('.').collect::<Vec<_>>();
    let valid_shape = matches!(segments.as_slice(), ["app", _])
        || matches!(segments.as_slice(), ["board", _, _, ..]);
    valid_shape
        && segments.iter().all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

#[cfg(test)]
mod tests {
    use super::{ACP_CHAT, FeatureSpec, FeatureStage, PI_CHAT, valid_id};

    #[test]
    fn acp_chat_is_an_opt_in_experiment() {
        assert!(valid_id(ACP_CHAT.id));
        assert_eq!(ACP_CHAT.stage, FeatureStage::Experimental);
        const {
            assert!(!ACP_CHAT.default_enabled);
            assert!(ACP_CHAT.toggleable);
        }
        assert!(ACP_CHAT.description.contains("permission requests"));
    }

    #[test]
    fn feature_ids_are_namespaced_ascii_segments() {
        assert!(valid_id("app.pi-chat"));
        assert!(valid_id("board.github.pull-requests.preview"));
        for invalid in [
            "",
            "pi-chat",
            "app",
            "app.two.parts",
            "board.github",
            "board..preview",
            "board.git_hub.preview",
            "board.github.white space",
            "board.github.é",
        ] {
            assert!(!valid_id(invalid), "{invalid}");
        }
        assert!(!valid_id(&format!("app.{}", "x".repeat(125))));
    }

    #[test]
    fn pi_chat_declares_the_initial_adapter_constraints() {
        assert_eq!(PI_CHAT.id, "app.pi-chat");
        assert_eq!(PI_CHAT.title, "Pi chat");
        assert_eq!(PI_CHAT.stage, FeatureStage::Experimental);
        const {
            assert!(!PI_CHAT.default_enabled);
            assert!(PI_CHAT.toggleable);
        }
        assert!(PI_CHAT.description.contains("Pi 1.0.4"));
        assert!(PI_CHAT.description.contains("without Copilot's sandbox"));
        let copied: FeatureSpec = PI_CHAT;
        assert_eq!(copied, PI_CHAT);
    }
}
