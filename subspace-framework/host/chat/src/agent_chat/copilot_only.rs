//! What a control only Copilot chat offers says when the running engine
//! cannot use it. A build with Copilot suggests choosing it in Settings; a
//! build without it says so instead, as Settings does not offer it there.

/// A chat control that only the Copilot engine offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CopilotOnly {
    Modes,
    Agents,
    Yolo,
    Replay,
}

/// Why the control is unavailable in this build.
pub(super) fn unavailable(control: CopilotOnly) -> &'static str {
    unavailable_in(control, subspace_agent::COPILOT_BUILT_IN)
}

/// Why the control is unavailable, in a build with or without Copilot.
fn unavailable_in(control: CopilotOnly, copilot_built_in: bool) -> &'static str {
    match (control, copilot_built_in) {
        (CopilotOnly::Modes, true) => {
            "Interaction modes are unavailable with Pi. Choose Copilot in Settings to use them."
        }
        (CopilotOnly::Modes, false) => {
            "Interaction modes need Copilot chat, which this build of Subspace does not include."
        }
        (CopilotOnly::Agents, true) => {
            "Custom agents are unavailable with Pi. Choose Copilot in Settings to use them."
        }
        (CopilotOnly::Agents, false) => {
            "Custom agents need Copilot chat, which this build of Subspace does not include."
        }
        (CopilotOnly::Yolo, true) => {
            "YOLO is unavailable with Pi. Choose Copilot in Settings to use it."
        }
        (CopilotOnly::Yolo, false) => {
            "YOLO needs Copilot chat, which this build of Subspace does not include."
        }
        (CopilotOnly::Replay, true) => {
            "Cloud-session replay is unavailable with Pi. Choose Copilot in Settings and restart Subspace to open this session."
        }
        (CopilotOnly::Replay, false) => {
            "Cloud-session replay needs Copilot chat, which this build of Subspace does not include. Open the task on GitHub instead."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{ChatPanel, InputIntent};
    use super::{CopilotOnly, unavailable, unavailable_in};
    use crate::prefs;
    use gpui_kit::TestAppContext;
    use subspace_core::config::ChatEngine;

    const CONTROLS: [CopilotOnly; 4] = [
        CopilotOnly::Modes,
        CopilotOnly::Agents,
        CopilotOnly::Yolo,
        CopilotOnly::Replay,
    ];

    #[test]
    fn a_build_without_copilot_does_not_suggest_choosing_it() {
        for control in CONTROLS {
            let offered = unavailable_in(control, true);
            assert!(offered.contains("Choose Copilot in Settings"), "{offered}");
            let missing = unavailable_in(control, false);
            assert!(
                missing.contains("this build of Subspace does not include"),
                "{missing}"
            );
            assert!(!missing.contains("Choose Copilot"), "{missing}");
        }
    }

    /// Pi refuses YOLO in its chat tab with the text for this build.
    #[gpui_kit::test]
    fn pi_refuses_yolo_with_the_text_for_this_build(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            prefs::init_file(None, cx);
        });
        let (chat, cx) =
            cx.add_window_view(|window, cx| ChatPanel::with_engine(ChatEngine::Pi, window, cx));
        chat.update(cx, |chat, _| {
            for intent in [InputIntent::EnableYolo, InputIntent::DisableYolo] {
                let submission = chat.prepare_submission(intent, Vec::new());
                assert!(submission.command.is_none());
                assert_eq!(
                    chat.tab().error.as_deref(),
                    Some(unavailable(CopilotOnly::Yolo))
                );
            }
        });
    }
}
