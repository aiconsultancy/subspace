//! Stateless issue compositions. Session ownership stays in IssueReferences.
mod accent;
mod activity;
mod facts;
mod label;
mod tracker;
pub(crate) use activity::relative;
pub(super) use label::label;
#[cfg(test)]
mod tests;

use super::tooltip::state_glyph;
use chrono::{DateTime, Utc};
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, Div, IntoElement as _, ParentElement as _, Styled as _, div};
use subspace_framework::contracts::issues::{IssueSummary, ResolutionState};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{
    Tokens,
    reference_card::{self, Treatment},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Variant {
    Accent,
    Tracker,
}
const CARD_WIDTH_REM: f32 = 400. / subspace_framework::dashboard::density::REFERENCE_REM;
const CHIP_TITLE_REM: f32 = 10.;
const TITLE_REM: f32 = 14. / subspace_framework::dashboard::density::REFERENCE_REM;
const TITLE_LINES: usize = 2;
/// The shared list skeleton's bar opacity (`dashboard::feedback::skeleton`).
const SKELETON_OPACITY: f32 = 0.6;
/// `reference_card::avatar`'s compact diameter as a share of the secondary
/// text size.
const CHIP_AVATAR_SCALE: f32 = 16. / 12.;

pub(super) fn variant(summary: &IssueSummary) -> Variant {
    if summary.tracker {
        Variant::Tracker
    } else {
        Variant::Accent
    }
}
/// The A/B compositions only appear in this passive hover surface.
pub(super) fn hover_card(
    summary: &IssueSummary,
    now: DateTime<Utc>,
    tokens: &Tokens,
    cx: &App,
) -> Div {
    let variant = variant(summary);
    reference_card::frame(
        content(summary, variant, now, tokens, cx),
        treatment(summary, variant),
        tokens,
        cx,
    )
    .w(gpui_kit::rems(CARD_WIDTH_REM))
}

pub(super) fn treatment(summary: &IssueSummary, variant: Variant) -> Treatment {
    match variant {
        Variant::Accent => Treatment::Accent(state_glyph(summary.state.kind).1),
        Variant::Tracker => Treatment::Outline,
    }
}
pub(super) fn content(
    summary: &IssueSummary,
    variant: Variant,
    now: DateTime<Utc>,
    tokens: &Tokens,
    cx: &App,
) -> AnyElement {
    match variant {
        Variant::Accent => accent::card(summary, now, tokens, cx).into_any_element(),
        Variant::Tracker => tracker::card(summary, now, tokens, cx).into_any_element(),
    }
}
pub(super) fn chip(summary: &IssueSummary, tokens: &Tokens, cx: &App) -> Div {
    let (glyph, tone) = state_glyph(summary.state.kind);
    chip_row(tokens)
        .child(
            Icon::from(glyph)
                .size(tokens.secondary_size())
                .text_color(tokens.tone(tone)),
        )
        .child(key(summary.identifier.clone(), tokens, cx))
        .child(
            div()
                .min_w_0()
                .max_w(gpui_kit::rems(CHIP_TITLE_REM))
                .text_ellipsis()
                .whitespace_nowrap()
                .child(summary.title.clone()),
        )
        .when_some(summary.assignee.as_deref(), |row, name| {
            row.child(reference_card::avatar(
                name,
                facts::is_agent(summary),
                true,
                tokens,
            ))
        })
}
/// What a chip's slot shows before its answer, or instead of a summary it
/// will not get: the same row as [`chip`], with the known key. A loading
/// slot draws static skeleton marks where the glyph and title go; an absent
/// one a muted glyph and a short reason.
pub(super) fn reserved(
    identifier: &str,
    state: Option<&ResolutionState>,
    tokens: &Tokens,
    cx: &App,
) -> Div {
    let key = key(identifier.to_owned(), tokens, cx).text_color(tokens.muted);
    match absence(state) {
        None => chip_row(tokens)
            .child(skeleton(tokens).size(tokens.secondary_size()))
            .child(key)
            .child(skeleton(tokens).h_2().w(gpui_kit::rems(CHIP_TITLE_REM))),
        Some(reason) => chip_row(tokens)
            .text_color(tokens.muted)
            .child(Icon::from(IconName::CircleSlash).size(tokens.secondary_size()))
            .child(key)
            .child(div().whitespace_nowrap().child(reason)),
    }
}
/// The short reason an answered reference has no summary; `None` while it
/// has no answer yet.
pub(super) fn absence(state: Option<&ResolutionState>) -> Option<&'static str> {
    match state {
        Some(ResolutionState::Unavailable) => Some("Unavailable"),
        Some(ResolutionState::SignedOut) => Some("Signed out"),
        Some(ResolutionState::Failed) => Some("Couldn't load"),
        _ => None,
    }
}
/// Static, like the shared list skeleton: nothing moves, so reduced motion
/// needs no case of its own.
fn skeleton(tokens: &Tokens) -> Div {
    div()
        .flex_shrink_0()
        .rounded_full()
        .bg(tokens.line.opacity(SKELETON_OPACITY))
}
/// Every chip row is at least as tall as the compact avatar, so a chip, its
/// placeholder and an unassigned chip all have one height.
fn chip_row(tokens: &Tokens) -> Div {
    h_flex()
        .min_w_0()
        .min_h(tokens.secondary_size() * CHIP_AVATAR_SCALE)
        .gap_1p5()
        .text_size(tokens.secondary_size())
}
fn key(label: String, tokens: &Tokens, cx: &App) -> Div {
    div()
        .flex_shrink_0()
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(tokens.secondary_size())
        .text_color(tokens.fg)
        .child(label)
}
fn state(summary: &IssueSummary, tokens: &Tokens) -> Div {
    let (glyph, tone) = state_glyph(summary.state.kind);
    reference_card::pill(summary.state.name.clone(), glyph, tone, tokens)
}
fn summary_text(summary: &IssueSummary, tokens: &Tokens) -> Option<Div> {
    const SUMMARY_LINES: usize = 3;
    use gpui_kit::base::TestTextExt as _;
    summary.summary.as_ref().map(|text| {
        div()
            .min_w_0()
            .whitespace_normal()
            .line_clamp(SUMMARY_LINES)
            .text_size(tokens.secondary_size())
            .text_color(tokens.muted)
            .child(text.clone().test_text(text.clone()))
    })
}
fn title(summary: &IssueSummary, tokens: &Tokens) -> Div {
    div()
        .min_w_0()
        .whitespace_normal()
        .line_clamp(TITLE_LINES)
        .font_weight(gpui_kit::FontWeight::MEDIUM)
        .text_size(gpui_kit::rems(TITLE_REM))
        .text_color(tokens.fg_strong)
        .child(summary.title.clone())
}
