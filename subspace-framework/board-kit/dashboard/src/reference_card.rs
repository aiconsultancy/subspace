//! Shared reference surfaces, composed from the native outline Button and Kit avatars.
use crate::{Tokens, Tone};
use gpui_kit::base::TestTextExt as _;
use gpui_kit::component::{
    Icon, Sizable as _, Size, ThemeStyled as _,
    avatar::Avatar,
    button::{Button, ButtonCustomVariant, ButtonRounded, ButtonVariants as _},
    h_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{App, Div, IntoElement, ParentElement as _, SharedString, Styled as _, div, rems};
use subspace_lucide::SvgIcon;

const TINT: f32 = 0.12;
const EDGE_REM: f32 = 3. / crate::density::REFERENCE_REM;
const CARD_PAD_Y_REM: f32 = 10. / crate::density::REFERENCE_REM;
/// Half a physical pixel is the approved hairline boundary.
const HAIRLINE_PX: f32 = 0.5;
const AVATAR_SCALE: f32 = 20. / 12.;
const CHIP_AVATAR_SCALE: f32 = 16. / 12.;

#[derive(Clone, Copy)]
pub enum Treatment {
    Outline,
    Accent(Tone),
    Chip,
}

/// Keep all activation and focus handling on the supplied native Button.
/// The state edge is a separate sibling so it cannot replace the focus border.
pub fn surface(button: Button, treatment: Treatment, tokens: &Tokens, cx: &App) -> Div {
    let variant = ButtonCustomVariant::new(cx)
        .foreground(tokens.fg)
        .hover(tokens.surface.blend(tokens.button_hover))
        .active(tokens.surface.blend(tokens.button_on))
        .shadow(false);
    let button = button
        .custom(variant)
        .border(gpui_kit::px(HAIRLINE_PX))
        .bg(tokens.surface)
        .border_color(tokens.line)
        .text_color(tokens.fg);
    let button = match treatment {
        Treatment::Outline => button
            .rounded(tokens.surface_radius)
            .py(rems(CARD_PAD_Y_REM)),
        Treatment::Accent(_) => button.rounded(ButtonRounded::None).py(rems(CARD_PAD_Y_REM)),
        Treatment::Chip => button.w_auto().rounded_full().px_2p5().py_1(),
    };
    state_edge(div().relative().min_w_0().child(button), treatment, tokens)
}

/// A passive reference surface for a tooltip view, with shared popover
/// elevation. Its edge is the popover's ring, so it adds no border of its own.
/// The caller supplies non-interactive content and width. Chips stay
/// interactive and use [`surface`]; the chip arm only keeps the match total.
pub fn frame(content: impl IntoElement, treatment: Treatment, tokens: &Tokens, cx: &App) -> Div {
    let frame = h_flex()
        .popover_style(cx)
        .relative()
        .min_w_0()
        .bg(tokens.surface)
        .text_color(tokens.fg)
        .text_sm()
        .px_3()
        .py(rems(CARD_PAD_Y_REM))
        .child(content);
    let frame = match treatment {
        Treatment::Outline => frame.rounded(tokens.surface_radius),
        Treatment::Accent(_) => frame.rounded_none(),
        Treatment::Chip => frame.rounded_full().px_2p5().py_1(),
    };
    state_edge(frame, treatment, tokens)
}

fn state_edge(surface: Div, treatment: Treatment, tokens: &Tokens) -> Div {
    surface.when_some(
        match treatment {
            Treatment::Accent(tone) => Some(tone),
            _ => None,
        },
        |surface, tone| {
            surface.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(rems(EDGE_REM))
                    .bg(tokens.tone(tone)),
            )
        },
    )
}

/// A semantic pill; the glyph preserves the meaning without colour.
pub fn pill(label: impl Into<SharedString>, glyph: SvgIcon, tone: Tone, tokens: &Tokens) -> Div {
    let label = label.into();
    h_flex()
        .gap_1()
        .px_2()
        .py_0p5()
        .rounded_full()
        .min_w_0()
        .bg(tokens.tone(tone).opacity(TINT))
        .text_color(tokens.tone(tone))
        .text_size(tokens.caption_size())
        .child(Icon::from(glyph).size(tokens.caption_size()))
        .child(
            div()
                .min_w_0()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(label.clone().test_text(label)),
        )
}

/// Initials' text size as a share of the avatar's diameter. GPUI Kit sizes the
/// initials' box, not their text, for a custom `Size::Size`, so inherited text
/// would overflow and clip (a 16px chip avatar drew "AF" as "A!").
const INITIALS_SCALE: f32 = 0.45;

/// A human's initials or an agent's anchor. Size follows interface zoom.
pub fn avatar(name: &str, agent: bool, compact: bool, tokens: &Tokens) -> impl IntoElement {
    let size = if compact {
        CHIP_AVATAR_SCALE
    } else {
        AVATAR_SCALE
    };
    let diameter = tokens.secondary_size() * size;
    Avatar::new()
        .with_size(Size::Size(diameter))
        .text_size(diameter * INITIALS_SCALE)
        .rounded_full()
        .bg(tokens.accent.opacity(TINT))
        .text_color(tokens.accent)
        .map(|avatar| {
            if agent {
                avatar.placeholder(
                    Icon::from(subspace_lucide::IconName::Anchor)
                        .size(tokens.caption_size())
                        .text_color(tokens.accent),
                )
            } else {
                avatar.name(name.to_owned())
            }
        })
}

#[cfg(test)]
mod tests;
