//! A token-themed Kit Tag with the approved issue-tile pill geometry.

use crate::{Tokens, Tone};
use gpui_kit::component::tag::Tag;
use gpui_kit::{
    App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, RenderOnce,
    Role, SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window,
    div,
};

const PADDING_X_PX: f32 = 8.0;
const PADDING_Y_PX: f32 = 2.0;
/// Kit Tag's line height, shared with the composer's height budget.
const LINE_HEIGHT: f32 = 1.25;
const PADDING_EDGES: f32 = 2.0;
/// The pill's fill: its tone at low opacity, as the issue tiles' pills are,
/// so it reads as a pill on any surface (the pane colour vanished in dark themes).
const TINT: f32 = 0.16;

fn padding(tokens: &Tokens, base: f32) -> Pixels {
    tokens.caption_size() * (base / super::FONT_SIZE_PX)
}

pub(crate) fn height(tokens: &Tokens) -> Pixels {
    tokens.caption_size() * LINE_HEIGHT + padding(tokens, PADDING_Y_PX) * PADDING_EDGES
}

/// Semantic emphasis shared by counts and issue states.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum PillTone {
    #[default]
    Muted,
    Warning,
    Danger,
}

fn foreground(tone: PillTone, tokens: &Tokens) -> gpui_kit::Hsla {
    match tone {
        PillTone::Muted => tokens.muted,
        PillTone::Warning => tokens.yellow,
        PillTone::Danger => tokens.red,
    }
}

/// The pill's text colour: its named [`Tone`] when it has one, else its
/// semantic [`PillTone`].
fn ink(tone: PillTone, hue: Option<Tone>, tokens: &Tokens) -> gpui_kit::Hsla {
    hue.map_or_else(|| foreground(tone, tokens), |hue| tokens.tone(hue))
}

/// A small, fully rounded status pill, reusable by any Space.
#[derive(IntoElement)]
pub struct Pill {
    id: ElementId,
    text: SharedString,
    label: SharedString,
    tone: PillTone,
    hue: Option<Tone>,
}

impl Pill {
    pub fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Self {
        let text = text.into();
        Self {
            id: id.into(),
            label: text.clone(),
            text,
            tone: PillTone::Muted,
            hue: None,
        }
    }

    pub fn tone(mut self, tone: PillTone) -> Self {
        self.tone = tone;
        self
    }

    /// Draws the pill in a named theme [`Tone`] rather than a semantic
    /// [`PillTone`], with the same tinted fill: a role mark such as a
    /// captain's in `Tone::Blue` or a rear admiral's in `Tone::Accent`.
    /// It takes precedence over [`Pill::tone`].
    pub fn hue(self, hue: Tone) -> Self {
        Self {
            hue: Some(hue),
            ..self
        }
    }

    /// A fuller status description for assistive technology.
    pub fn aria_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }
}

impl RenderOnce for Pill {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let foreground = ink(self.tone, self.hue, &tokens);
        let fill = background(foreground);
        div()
            .id(self.id)
            .test_support()
            .role(Role::Status)
            .aria_label(self.label)
            .flex_shrink_0()
            .child(
                Tag::custom(fill, foreground, fill)
                    .border_0()
                    .rounded_full()
                    .px(padding(&tokens, PADDING_X_PX))
                    .py(padding(&tokens, PADDING_Y_PX))
                    .text_size(tokens.caption_size())
                    .child(self.text),
            )
    }
}

/// The tinted fill behind a pill's text.
fn background(foreground: gpui_kit::Hsla) -> gpui_kit::Hsla {
    foreground.opacity(TINT)
}

#[cfg(test)]
mod tests {
    use super::{
        PADDING_X_PX, PADDING_Y_PX, Pill, PillTone, TINT, background, foreground, height, ink,
        padding,
    };
    use crate::{Tokens, Tone};
    use gpui_kit::px;

    #[test]
    fn pill_geometry_and_colours_follow_the_caption_and_semantic_tokens() {
        let tokens = Tokens::with_density(&crate::density::Density::default());
        assert_eq!(padding(&tokens, PADDING_X_PX), px(8.0));
        assert_eq!(padding(&tokens, PADDING_Y_PX), px(2.0));
        assert_eq!(height(&tokens), px(17.75));
        assert_eq!(foreground(PillTone::Muted, &tokens), tokens.muted);
        assert_eq!(foreground(PillTone::Warning, &tokens), tokens.yellow);
        assert_eq!(foreground(PillTone::Danger, &tokens), tokens.red);
        assert_eq!(background(tokens.yellow), tokens.yellow.opacity(TINT));
        assert_ne!(background(tokens.muted), tokens.pane, "visible on the pane");
    }

    #[test]
    fn a_named_hue_takes_its_theme_colour_over_the_semantic_tone() {
        let tokens = Tokens::with_density(&crate::density::Density::default());
        assert_eq!(ink(PillTone::Danger, None, &tokens), tokens.red);
        assert_eq!(
            ink(PillTone::Danger, Some(Tone::Blue), &tokens),
            tokens.blue
        );
        assert_eq!(
            ink(PillTone::Muted, Some(Tone::Accent), &tokens),
            tokens.accent
        );
        let pill = Pill::new("rank", "CAPTAIN")
            .tone(PillTone::Warning)
            .hue(Tone::Blue);
        assert_eq!((pill.tone, pill.hue), (PillTone::Warning, Some(Tone::Blue)));
    }
}
