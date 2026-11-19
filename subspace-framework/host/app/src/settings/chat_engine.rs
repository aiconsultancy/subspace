//! The chat engine and ACP agent choices on the Chat page. The agent applies
//! only when the engine is ACP, and the page shows it only then.
use super::form::{Choice, choice};
use gpui_kit::{App, Window};
use subspace_core::config::{AcpAgent, ChatEngine};
use subspace_core::features::{ACP_CHAT, FeatureSpec, FeatureStage, PI_CHAT};

pub(super) fn chat_engine_from_label(label: Option<&str>) -> ChatEngine {
    if label.is_some_and(|label| label == pi_engine_label(&PI_CHAT)) {
        ChatEngine::Pi
    } else if label.is_some_and(|label| label == acp_engine_label(&ACP_CHAT)) {
        ChatEngine::Acp
    } else if label == Some(ChatEngine::Copilot.label()) {
        ChatEngine::Copilot
    } else {
        ChatEngine::Off
    }
}

pub(super) fn chat_engine_choice(engine: ChatEngine, window: &mut Window, cx: &mut App) -> Choice {
    let pi_label = pi_engine_label(&PI_CHAT);
    let acp_label = acp_engine_label(&ACP_CHAT);
    let mut choices = chat_engine_labels(&PI_CHAT);
    if ACP_CHAT.stage != FeatureStage::Removed {
        choices.push(acp_label.clone());
    }
    let selected = match (engine, PI_CHAT.stage, ACP_CHAT.stage) {
        (ChatEngine::Pi, FeatureStage::Experimental | FeatureStage::Stable, _) => pi_label,
        (ChatEngine::Acp, _, FeatureStage::Experimental | FeatureStage::Stable) => acp_label,
        (ChatEngine::Copilot, _, _) => ChatEngine::Copilot.label().into(),
        _ => ChatEngine::Off.label().into(),
    };
    choice(choices, Some(&selected), false, window, cx)
}

fn chat_engine_labels(spec: &FeatureSpec) -> Vec<String> {
    let mut labels = vec![
        ChatEngine::Off.label().into(),
        ChatEngine::Copilot.label().into(),
    ];
    if spec.stage != FeatureStage::Removed {
        labels.push(pi_engine_label(spec));
    }
    labels
}

fn pi_engine_label(spec: &FeatureSpec) -> String {
    match spec.stage {
        FeatureStage::Experimental => "Pi (Experimental)".into(),
        FeatureStage::Stable | FeatureStage::Removed => "Pi".into(),
    }
}

pub(super) fn validate_chat_feature(
    engine: ChatEngine,
    pi_enabled: bool,
    spec: &FeatureSpec,
) -> Result<(), String> {
    if engine == ChatEngine::Pi && !pi_enabled {
        Err(pi_save_error(spec))
    } else {
        Ok(())
    }
}

fn pi_save_error(spec: &FeatureSpec) -> String {
    match (spec.stage, spec.toggleable) {
        (FeatureStage::Experimental, true) => {
            "Pi is experimental and disabled. Enable it in Experiments or choose another engine."
                .into()
        }
        (FeatureStage::Stable, true) => {
            "Pi is disabled. Enable it in General or choose another engine.".into()
        }
        (FeatureStage::Removed, _) => "Pi has been removed. Choose another engine.".into(),
        (_, false) => "Pi is unavailable. Choose another engine.".into(),
    }
}

pub(super) fn acp_engine_label(spec: &FeatureSpec) -> String {
    match spec.stage {
        FeatureStage::Experimental => "ACP agent (Experimental)".into(),
        FeatureStage::Stable | FeatureStage::Removed => ChatEngine::Acp.label().into(),
    }
}

pub(super) fn acp_agent_choice(agent: AcpAgent, window: &mut Window, cx: &mut App) -> Choice {
    let labels = AcpAgent::ALL
        .iter()
        .map(|agent| agent.label().to_owned())
        .collect();
    choice(labels, Some(agent.label()), false, window, cx)
}

pub(super) fn acp_agent_from_label(label: Option<&str>) -> AcpAgent {
    AcpAgent::ALL
        .into_iter()
        .find(|agent| Some(agent.label()) == label)
        .unwrap_or_default()
}

pub(super) fn validate_acp_feature(
    engine: ChatEngine,
    enabled: bool,
    spec: &FeatureSpec,
) -> Result<(), String> {
    if engine != ChatEngine::Acp || enabled {
        return Ok(());
    }
    Err(match spec.stage {
        FeatureStage::Experimental => {
            "ACP agent chat is experimental and disabled. Enable it in Experiments or choose another engine."
        }
        FeatureStage::Stable => "ACP agent chat is disabled. Enable it in General or choose another engine.",
        FeatureStage::Removed => "ACP agent chat has been removed. Choose another engine.",
    }
    .into())
}

#[cfg(test)]
mod tests {
    use super::{
        acp_agent_from_label, acp_engine_label, chat_engine_from_label, chat_engine_labels,
        pi_engine_label, pi_save_error, validate_acp_feature, validate_chat_feature,
    };
    use subspace_core::config::{AcpAgent, ChatEngine};
    use subspace_core::features::{ACP_CHAT, FeatureSpec, FeatureStage, PI_CHAT};

    #[test]
    fn chat_engine_labels_map_to_safe_choices() {
        assert_eq!(chat_engine_from_label(Some("Copilot")), ChatEngine::Copilot);
        assert_eq!(
            chat_engine_from_label(Some("Pi (Experimental)")),
            ChatEngine::Pi
        );
        assert_eq!(chat_engine_from_label(Some("Off")), ChatEngine::Off);
        assert_eq!(chat_engine_from_label(None), ChatEngine::Off);
        assert_eq!(chat_engine_from_label(Some("unknown")), ChatEngine::Off);
    }

    #[test]
    fn pi_labels_and_save_policy_follow_the_registered_stage() {
        assert_eq!(pi_engine_label(&PI_CHAT), "Pi (Experimental)");
        let stable = FeatureSpec {
            stage: FeatureStage::Stable,
            ..PI_CHAT
        };
        assert_eq!(pi_engine_label(&stable), "Pi");
        assert_eq!(chat_engine_labels(&stable), ["Off", "Copilot", "Pi"]);
        let removed = FeatureSpec {
            stage: FeatureStage::Removed,
            ..PI_CHAT
        };
        assert_eq!(chat_engine_labels(&removed), ["Off", "Copilot"]);
        assert!(validate_chat_feature(ChatEngine::Copilot, false, &PI_CHAT).is_ok());
        assert!(validate_chat_feature(ChatEngine::Pi, true, &PI_CHAT).is_ok());
        assert!(pi_save_error(&PI_CHAT).contains("Experiments"));
        assert!(pi_save_error(&stable).contains("General"));
        assert!(!pi_save_error(&stable).contains("Experiments"));
        assert!(pi_save_error(&removed).contains("removed"));
        assert!(!pi_save_error(&removed).contains("Enable"));
    }

    #[test]
    fn the_acp_label_selects_the_acp_engine() {
        assert_eq!(
            chat_engine_from_label(Some("ACP agent (Experimental)")),
            ChatEngine::Acp
        );
    }

    #[test]
    fn agent_labels_map_back_to_their_preset() {
        assert_eq!(acp_agent_from_label(Some("Claude Code")), AcpAgent::Claude);
        assert_eq!(acp_agent_from_label(Some("Gemini CLI")), AcpAgent::Gemini);
        assert_eq!(acp_agent_from_label(None), AcpAgent::OpenCode);
        assert_eq!(acp_agent_from_label(Some("unknown")), AcpAgent::OpenCode);
    }

    #[test]
    fn the_acp_engine_needs_its_experiment() {
        assert_eq!(acp_engine_label(&ACP_CHAT), "ACP agent (Experimental)");
        let stable = FeatureSpec {
            stage: FeatureStage::Stable,
            ..ACP_CHAT
        };
        assert_eq!(acp_engine_label(&stable), "ACP agent");
        assert!(validate_acp_feature(ChatEngine::Acp, true, &ACP_CHAT).is_ok());
        assert!(validate_acp_feature(ChatEngine::Copilot, false, &ACP_CHAT).is_ok());
        let error = validate_acp_feature(ChatEngine::Acp, false, &ACP_CHAT).unwrap_err();
        assert!(error.contains("Experiments"), "{error}");
    }
}
