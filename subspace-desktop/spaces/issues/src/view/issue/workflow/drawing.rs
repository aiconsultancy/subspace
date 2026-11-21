use gpui_kit::base::ElementExt as _;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Div, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, rems,
};
use subspace_api_client::model::{IssueDetail, WorkflowState};
use subspace_framework::dashboard::Tokens;

use super::{FlightPath, Layout, Segment, end};
use crate::{Issues, model::StateKind, view::lanes};

/// The rule's thickness and the gaps, in rem so they follow the text size.
const RULE_REM: f32 = 0.1875;
const GAP_REM: f32 = 0.375;
const BASE_REM: f32 = 16.;

pub(in crate::view::issue) fn render(
    issues: &Issues,
    detail: &IssueDetail,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Option<AnyElement> {
    let path = super::path(detail, &issues.projects)?;
    let scale = f32::from(cx.theme().font_size) / BASE_REM;
    let body = match path.layout(issues.issue_view.workflow_width, scale) {
        Layout::Current => compact(&path, tokens),
        Layout::Segments { filled } => {
            let end = end::slot(issues, detail, &path, tokens, cx);
            bar(&path, filled, end, tokens).into_any_element()
        }
    };
    let owner = cx.weak_entity();
    Some(
        v_flex()
            .id("issue-workflow")
            .aria_label(path.description())
            .test_support()
            .on_prepaint(move |bounds, _, cx| {
                let owner = owner.clone();
                cx.defer(move |cx| {
                    _ = owner.update(cx, |issues, cx| {
                        let measured = f32::from(bounds.size.width);
                        if subspace_framework::dashboard::measure::changed(
                            issues.issue_view.workflow_width,
                            measured,
                        ) {
                            issues.issue_view.workflow_width = measured;
                            cx.notify();
                        }
                    });
                });
            })
            .w_full()
            .child(body)
            .into_any_element(),
    )
}

/// Too narrow for the bar: the current state's glyph and name, and its place.
fn compact(path: &FlightPath<'_>, tokens: &Tokens) -> AnyElement {
    let state = path.selected();
    h_flex()
        .id("issue-workflow-current")
        .test_support()
        .gap_2()
        .text_size(tokens.secondary_size())
        .children(state.map(|state| {
            let (glyph, tone) = lanes::state_icon(StateKind::parse(&state.kind));
            Icon::from(glyph).xsmall().text_color(tokens.tone(tone))
        }))
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(
                    state.map_or_else(|| "Current state unavailable".into(), |s| s.name.clone()),
                ),
        )
        .children(path.current.map(|ix| {
            div()
                .text_color(tokens.muted)
                .child(format!("{} of {}", ix + 1, path.count()))
        }))
        .into_any_element()
}

/// The path steps and the end slot share the row equally. The first
/// `filled` segments are filled.
fn bar(path: &FlightPath<'_>, filled: usize, end: Option<AnyElement>, tokens: &Tokens) -> Div {
    h_flex()
        .w_full()
        .min_w_0()
        .items_start()
        .gap(rems(GAP_REM))
        .children(path.course.iter().enumerate().map(|(ix, state)| {
            let segment = Segment {
                filled: ix < filled,
                current: path.current == Some(ix),
            };
            step(state, segment, tokens)
        }))
        .children(end)
}

/// One stop's equal share of the bar: its rule above its name.
pub(super) fn cell() -> Div {
    v_flex().flex_1().min_w_0().gap(rems(GAP_REM))
}

/// A stop that only shows: its rule and its name.
pub(super) fn step(state: &WorkflowState, segment: Segment, tokens: &Tokens) -> Div {
    cell()
        .child(rule(segment.filled, tokens))
        .child(caption(&state.name, segment.current, tokens))
}

/// The thin rule: filled up to and including the current stop.
pub(super) fn rule(filled: bool, tokens: &Tokens) -> Div {
    div()
        .w_full()
        .h(rems(RULE_REM))
        .rounded_full()
        .bg(if filled { tokens.accent } else { tokens.line })
}

/// A stop's name: strong at the current stop, quiet elsewhere.
pub(super) fn caption(name: &str, current: bool, tokens: &Tokens) -> Div {
    div()
        .min_w_0()
        .truncate()
        .text_size(tokens.caption_size())
        .text_color(if current {
            tokens.fg_strong
        } else {
            tokens.muted
        })
        .when(current, |caption| caption.font_weight(FontWeight::SEMIBOLD))
        .child(name.to_owned())
}
