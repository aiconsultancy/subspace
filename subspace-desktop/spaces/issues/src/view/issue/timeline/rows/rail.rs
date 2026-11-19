//! The timeline's rail and markers borrow the workflow indicator's shapes.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Div, FontWeight, Hsla, InteractiveElement as _, ParentElement as _, Styled as _,
    TestSupportExt as _, div,
};
use subspace_framework::dashboard::Tokens;

const RAIL_OPACITY: f32 = 0.42;

pub(super) fn date(label: &str, tokens: &Tokens) -> Div {
    h_flex()
        .gap_3()
        .pb_2()
        .child(div().w_4().flex_shrink_0())
        .child(
            div()
                .id(format!("timeline-date:{label}"))
                .test_support()
                .flex_shrink_0()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.muted)
                .child(label.to_owned()),
        )
        .child(div().h_px().min_w_0().flex_1().bg(tokens.line))
}

pub(super) fn entry(
    id: &str,
    body: Div,
    comment: bool,
    tokens: &Tokens,
) -> impl gpui_kit::IntoElement {
    h_flex()
        .id(format!("timeline-entry:{id}"))
        .test_support()
        .min_w_0()
        .items_start()
        .gap_3()
        .child(track(comment, tokens))
        .child(body.flex_1().pb_3().when(comment, |body| body.pb_4()))
}

fn track(comment: bool, tokens: &Tokens) -> Div {
    let line = tokens.muted.opacity(RAIL_OPACITY);
    v_flex()
        .w_4()
        .flex_shrink_0()
        .self_stretch()
        .items_center()
        .child(div().w_px().h_1().bg(line))
        .child(marker(comment, tokens, line))
        .child(div().w_px().flex_1().bg(line))
}

fn marker(comment: bool, tokens: &Tokens, line: Hsla) -> Div {
    h_flex()
        .size_3()
        .justify_center()
        .rounded_full()
        .bg(tokens.pane)
        .border_1()
        .border_color(if comment { tokens.accent } else { line })
        .when(comment, |marker| {
            marker.child(div().size_1().rounded_full().bg(tokens.accent))
        })
}
