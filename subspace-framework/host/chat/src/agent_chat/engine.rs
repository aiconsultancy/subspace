//! Engine-specific capabilities, names, marks and user guidance for the
//! shared panel.
use super::{ChatEngine, ChatPanel};
use subspace_agent::AcpAgent;
use subspace_core::features::{FeatureSpec, FeatureStage, PI_CHAT};
use subspace_lucide::{IconName, SvgIcon};

// Product marks identify the engine that runs the chat. Lucide carries no
// brand marks; assets/PROVENANCE.md records each file's source and licence.
const COPILOT_MARK: SvgIcon = SvgIcon::new(include_bytes!("../../assets/copilot.svg"));
const OPENAI_MARK: SvgIcon = SvgIcon::new(include_bytes!("../../assets/openai.svg"));
const CLAUDE_MARK: SvgIcon = SvgIcon::new(include_bytes!("../../assets/claude.svg"));
const OPENCODE_MARK: SvgIcon = SvgIcon::new(include_bytes!("../../assets/opencode.svg"));
const GEMINI_MARK: SvgIcon = SvgIcon::new(include_bytes!("../../assets/gemini.svg"));

const COPILOT_GUIDANCE: &str = concat!(
    "Ask about your work. Copilot uses the GitHub account chosen in Settings > GitHub. ",
    "Never paste saved service tokens into chat."
);
const COPILOT_AUTHORIZATION_GUIDANCE: &str = concat!(
    "Subspace chat needs Copilot CLI/SDK authorization, separate from VS Code Copilot. ",
    "If your organization blocks this access, contact your administrator. ",
    "Reconnect after access is approved."
);
const PI_GUIDANCE: &str = concat!(
    "Ask about your work. Pi uses its existing provider authentication. ",
    "Never paste saved service tokens into chat."
);
const ACP_GUIDANCE: &str = concat!(
    "Ask about your work. The agent uses its own sign-in, tools and permission rules; ",
    "approve its requests here. Never paste saved service tokens into chat."
);
const PI_EXPERIMENTAL_GUIDANCE: &str = concat!(
    "Pi is experimental and must be enabled in Settings > Experiments. ",
    "It uses its existing provider authentication. Never paste saved service tokens into chat."
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EngineCapabilities {
    pub(super) copilot_controls: bool,
}

impl EngineCapabilities {
    fn for_engine(engine: ChatEngine) -> Self {
        Self {
            copilot_controls: engine == ChatEngine::Copilot,
        }
    }

    pub(super) fn cloud_replay(self) -> bool {
        self.copilot_controls
    }

    pub(super) fn ai_units(self) -> bool {
        self.copilot_controls
    }
}

impl ChatPanel {
    pub fn engine(&self) -> ChatEngine {
        self.engine
    }

    pub fn is_disabled(&self) -> bool {
        self.engine == ChatEngine::Off
    }

    pub fn engine_label(&self) -> &'static str {
        engine_label(self.engine, self.acp_agent)
    }

    /// The mark shown beside the engine's name wherever the app names it.
    pub fn engine_icon(&self) -> SvgIcon {
        engine_icon(self.engine, self.acp_agent)
    }

    pub(super) fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities::for_engine(self.engine)
    }

    pub(super) fn guidance(&self) -> &'static str {
        match self.engine {
            ChatEngine::Off if self.copilot_kept => subspace_agent::COPILOT_UNAVAILABLE,
            ChatEngine::Off => {
                "Choose an engine in Settings > Chat, then save and restart Subspace."
            }
            ChatEngine::Copilot => COPILOT_GUIDANCE,
            ChatEngine::Pi if PI_CHAT.stage == FeatureStage::Experimental => {
                PI_EXPERIMENTAL_GUIDANCE
            }
            ChatEngine::Pi => PI_GUIDANCE,
            ChatEngine::Acp => ACP_GUIDANCE,
        }
    }
}

/// The engine the panel runs for the configured one, and whether that keeps
/// a Copilot choice this build cannot run. A build without Copilot runs none
/// for it: the panel is off, as with no engine, and nothing starts, signs in
/// or shows Copilot.
pub(super) fn runnable(configured: ChatEngine, copilot_built_in: bool) -> (ChatEngine, bool) {
    match configured {
        ChatEngine::Copilot if !copilot_built_in => (ChatEngine::Off, true),
        engine => (engine, false),
    }
}

/// The name the panel and its composer use for the engine.
pub(super) fn engine_label(engine: ChatEngine, agent: AcpAgent) -> &'static str {
    match engine {
        ChatEngine::Acp => agent.engine_label(),
        _ => pi_label(engine, &PI_CHAT),
    }
}

/// The product mark for the engine, or a neutral glyph for Pi and a custom
/// ACP agent.
fn engine_icon(engine: ChatEngine, agent: AcpAgent) -> SvgIcon {
    match (engine, agent) {
        (ChatEngine::Off, _) => IconName::Bot,
        (ChatEngine::Copilot, _) => COPILOT_MARK,
        (ChatEngine::Pi, _) => IconName::Bot,
        (ChatEngine::Acp, AcpAgent::OpenCode) => OPENCODE_MARK,
        (ChatEngine::Acp, AcpAgent::Claude) => CLAUDE_MARK,
        (ChatEngine::Acp, AcpAgent::Codex) => OPENAI_MARK,
        (ChatEngine::Acp, AcpAgent::Gemini) => GEMINI_MARK,
        (ChatEngine::Acp, AcpAgent::Custom) => IconName::Plug,
    }
}

fn pi_label(engine: ChatEngine, pi: &FeatureSpec) -> &'static str {
    match (engine, pi.stage) {
        (ChatEngine::Off, _) => "Chat",
        (ChatEngine::Copilot, _) => "Copilot",
        (ChatEngine::Pi, FeatureStage::Experimental) => "Pi (Experimental)",
        (ChatEngine::Pi, FeatureStage::Stable | FeatureStage::Removed) => "Pi",
        (ChatEngine::Acp, _) => ChatEngine::Acp.label(),
    }
}

/// Authorization guidance belongs to a Copilot runtime that stopped and can
/// be reconnected, not to an error inside a running chat.
pub(super) fn error_guidance(engine: ChatEngine, reconnect: bool) -> Option<&'static str> {
    (engine == ChatEngine::Copilot && reconnect).then_some(COPILOT_AUTHORIZATION_GUIDANCE)
}

#[cfg(test)]
mod tests {
    use super::{
        AcpAgent, CLAUDE_MARK, COPILOT_MARK, ChatEngine, EngineCapabilities, GEMINI_MARK,
        OPENAI_MARK, OPENCODE_MARK, engine_icon, engine_label, error_guidance, pi_label, runnable,
    };
    use gpui_kit::{DevicePixels, SvgSize, TestAppContext, size};
    use subspace_core::features::{FeatureSpec, FeatureStage, PI_CHAT};
    use subspace_lucide::IconName;

    const MARKS: [super::SvgIcon; 5] = [
        COPILOT_MARK,
        OPENAI_MARK,
        CLAUDE_MARK,
        OPENCODE_MARK,
        GEMINI_MARK,
    ];

    #[test]
    fn every_engine_and_agent_is_shown_with_its_own_mark() {
        assert_eq!(
            engine_icon(ChatEngine::Copilot, AcpAgent::Codex),
            COPILOT_MARK
        );
        assert_eq!(engine_icon(ChatEngine::Pi, AcpAgent::Claude), IconName::Bot);
        let acp = AcpAgent::ALL.map(|agent| engine_icon(ChatEngine::Acp, agent));
        assert_eq!(
            acp,
            [
                OPENCODE_MARK,
                CLAUDE_MARK,
                OPENAI_MARK,
                GEMINI_MARK,
                IconName::Plug
            ]
        );
        for (index, mark) in MARKS.iter().enumerate() {
            assert!(
                MARKS[index + 1..].iter().all(|other| other != mark),
                "mark {index} is not distinct"
            );
        }
    }

    #[gpui_kit::test]
    fn every_mark_is_a_tintable_svg_that_renders_at_glyph_size(cx: &mut TestAppContext) {
        let renderer = cx.update(|cx| cx.svg_renderer());
        for mark in MARKS {
            let text = std::str::from_utf8(mark.bytes()).unwrap();
            assert!(text.contains("fill=\"currentColor\""), "{text}");
            assert!(!text.contains('#'), "a mark must not fix its own colour");
            let svg = renderer.parse_svg(mark.bytes()).unwrap();
            let glyph = size(DevicePixels(16), DevicePixels(16));
            let image = renderer
                .render_parsed(&svg, SvgSize::ExactSize(glyph))
                .unwrap();
            assert_eq!(image.size(0), glyph);
            let (pixels, _) = image.as_bytes(0).unwrap().as_chunks::<4>();
            assert!(pixels.iter().filter(|pixel| pixel[3] > 0).count() > 16);
        }
    }

    #[test]
    fn local_copilot_errors_explain_authorization_without_diagnosing_a_policy_block() {
        let guidance = error_guidance(ChatEngine::Copilot, true).unwrap();
        assert!(guidance.contains("Copilot CLI/SDK authorization, separate from VS Code Copilot"));
        assert!(
            guidance
                .contains("If your organization blocks this access, contact your administrator")
        );
        assert!(
            error_guidance(ChatEngine::Copilot, false).is_none(),
            "an error in a running chat is not an authorization problem"
        );
        assert!(error_guidance(ChatEngine::Pi, false).is_none());
        assert!(error_guidance(ChatEngine::Pi, true).is_none());
    }

    #[test]
    fn pi_keeps_shared_chat_features_but_not_copilot_controls() {
        let pi = EngineCapabilities::for_engine(ChatEngine::Pi);
        assert!(!pi.copilot_controls);
        assert!(!pi.cloud_replay());
        assert!(!pi.ai_units());

        let copilot = EngineCapabilities::for_engine(ChatEngine::Copilot);
        assert!(copilot.copilot_controls);
        assert!(copilot.cloud_replay());
        assert!(copilot.ai_units());
    }

    #[test]
    fn a_build_without_copilot_runs_no_engine_for_a_saved_copilot_choice() {
        assert_eq!(
            runnable(ChatEngine::Copilot, false),
            (ChatEngine::Off, true)
        );
        assert_eq!(
            runnable(ChatEngine::Copilot, true),
            (ChatEngine::Copilot, false)
        );
        for engine in [ChatEngine::Off, ChatEngine::Pi, ChatEngine::Acp] {
            assert_eq!(runnable(engine, false), (engine, false));
            assert_eq!(runnable(engine, true), (engine, false));
        }
    }

    #[test]
    fn pi_branding_follows_its_registered_lifecycle() {
        assert_eq!(pi_label(ChatEngine::Pi, &PI_CHAT), "Pi (Experimental)");
        let stable = FeatureSpec {
            stage: FeatureStage::Stable,
            ..PI_CHAT
        };
        assert_eq!(pi_label(ChatEngine::Pi, &stable), "Pi");
        assert_eq!(pi_label(ChatEngine::Copilot, &PI_CHAT), "Copilot");
    }

    #[test]
    fn acp_chat_is_named_after_its_agent() {
        let names = AcpAgent::ALL.map(|agent| engine_label(ChatEngine::Acp, agent));
        assert_eq!(
            names,
            ["OpenCode", "Claude", "Codex", "Gemini", "ACP agent"]
        );
        assert_eq!(
            engine_label(ChatEngine::Copilot, AcpAgent::Claude),
            "Copilot"
        );
        let acp = EngineCapabilities::for_engine(ChatEngine::Acp);
        assert!(!acp.copilot_controls);
        assert!(error_guidance(ChatEngine::Acp, true).is_none());
    }
}
