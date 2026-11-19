//! The passive frame keeps the card geometry and one themed elevated surface.
use super::{CARD_PAD_Y_REM, Treatment, frame};
use crate::{Tokens, Tone};
use gpui_kit::{Element as _, IntoElement as _, Styled as _, TestAppContext, div, px, rems};

#[gpui_kit::test]
fn passive_frames_keep_treatment_geometry_with_the_popover_ring_as_their_edge(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    cx.update(|cx| {
        let tokens = Tokens::one_dark();
        for (treatment, radius) in [
            (Treatment::Outline, tokens.surface_radius),
            (Treatment::Accent(Tone::Yellow), px(0.)),
        ] {
            let mut element = frame(div(), treatment, &tokens, cx);
            let style = element.style();
            assert_eq!(style.background, Some(tokens.surface.into()));
            assert_eq!(style.border_widths.top, None);
            assert_eq!(style.padding.top, Some(rems(CARD_PAD_Y_REM).into()));
            assert_eq!(style.corner_radii.top_left, Some(radius.into()));
            assert!(
                style
                    .box_shadow
                    .as_ref()
                    .is_some_and(|shadows| !shadows.is_empty())
            );
            assert!(element.into_element().a11y_role().is_none());
        }
    });
}
