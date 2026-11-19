//! The line under the transcript that says what Copilot is up to, which
//! changes every few seconds while a turn runs. The phrases themselves and
//! their order live in `status_words`.

use std::time::Duration;

use gpui_kit::{
    AnimationExt as _, AnyElement, Hsla, IntoElement as _, ParentElement as _, Styled as _,
};

use super::status_words;
use super::{AgentHandle, Animation, ChatPanel, Tokens, div};

/// How long the busy status line takes to complete one accent shimmer.
const SHIMMER_DURATION: Duration = Duration::from_millis(1_800);
const UNIT_MIN: f32 = 0.;
const UNIT_MAX: f32 = 1.;
const PEAK_PHASE: f32 = 0.5;
const LIGHTNESS_BOOST: f32 = 0.04;

fn shimmer_color(accent: Hsla, phase: f32) -> Hsla {
    let phase = phase.clamp(UNIT_MIN, UNIT_MAX);
    let brightness = if phase <= PEAK_PHASE {
        phase / PEAK_PHASE
    } else {
        (UNIT_MAX - phase) / PEAK_PHASE
    };
    Hsla {
        l: (accent.l + brightness * LIGHTNESS_BOOST).min(UNIT_MAX),
        ..accent
    }
}

impl ChatPanel {
    /// The phrase changes every few seconds and shimmers while the turn runs.
    /// GPUI leaves the line static when the system asks for reduced motion.
    pub(super) fn render_status(&self, tokens: &Tokens) -> Option<AnyElement> {
        let turn = self.tab().running()?;
        let phrase = status_words::phrase(turn.phrase);
        let accent = tokens.accent;
        Some(
            div()
                .w_full()
                .text_center()
                .whitespace_normal()
                .text_color(accent)
                .child(format!("{} is {phrase}…", self.engine_label()))
                .with_animation(
                    ("chat-status", turn.phrase as u64),
                    Animation::new(SHIMMER_DURATION).repeat(),
                    move |line, delta| line.text_color(shimmer_color(accent, delta)),
                )
                .into_any_element(),
        )
    }

    /// Whether the chat runtime failed to start or stopped. Before the panel
    /// first opens there is no runtime and no failure, which is not this.
    pub fn is_unavailable(&self) -> bool {
        runtime_unavailable(
            self.handle.as_ref().map(AgentHandle::is_stopped),
            self.error.is_some(),
        )
    }

    /// Ends every tab's turn once the runtime has stopped, as after an
    /// unexpected stop, so no tab stays busy with Reconnect disabled.
    /// Returns whether a turn ended.
    pub(super) fn end_stopped_turns(&mut self) -> bool {
        let stopped = self.is_unavailable();
        super::attention::end_turns(&mut self.tabs, stopped)
    }
}

/// `stopped` is `None` while no runtime was started.
fn runtime_unavailable(stopped: Option<bool>, failed: bool) -> bool {
    stopped.unwrap_or(failed)
}

#[cfg(test)]
mod tests {
    use super::{runtime_unavailable, shimmer_color};
    use gpui_kit::Hsla;

    #[test]
    fn only_a_stopped_or_unstartable_runtime_is_unavailable() {
        assert!(!runtime_unavailable(None, false), "chat not opened yet");
        assert!(
            runtime_unavailable(None, true),
            "the runtime could not start"
        );
        assert!(runtime_unavailable(Some(true), false));
        assert!(
            !runtime_unavailable(Some(false), true),
            "a running chat with a passing error is still available"
        );
    }

    #[test]
    fn shimmer_preserves_accent_hue_and_returns_to_base_color() {
        let accent = Hsla {
            h: 0.52,
            s: 0.8,
            l: 0.5,
            a: 1.,
        };

        assert_eq!(shimmer_color(accent, 0.), accent);
        assert_eq!(shimmer_color(accent, 1.), accent);
        assert_eq!(shimmer_color(accent, 0.5).h, accent.h);
        assert!(shimmer_color(accent, 0.5).l > accent.l);
    }
}
