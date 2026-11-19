use gpui_kit::base::ElementExt as _;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Div, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, canvas, div, fill,
    point, px, size,
};
use subspace_api_client::model::{IssueDetail, WorkflowState};
use subspace_framework::dashboard::Tokens;

use super::{FlightPath, Layout, end};
use crate::{Issues, model::StateKind, view::lanes};

// Drawing geometry is expressed in rem units and resolved afresh each frame.
const BEACON_REM: f32 = 2.;
const COURSE_HEIGHT_REM: f32 = 2.;
const DOT_STEP_REM: f32 = 0.5;
const DOT_REM: f32 = 0.125;
const BASE_REM: f32 = 16.;
const ZERO: f32 = 0.;
const HALF: f32 = 0.5;
const HAIRLINE: f32 = 1.;

pub(in crate::view::issue) fn render(
    issues: &Issues,
    detail: &IssueDetail,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Option<AnyElement> {
    let path = super::path(detail, &issues.projects)?;
    let scale = f32::from(cx.theme().font_size) / BASE_REM;
    let body = match path.layout(issues.issue_view.workflow_width, scale) {
        Layout::Current => compact(&path, tokens, cx),
        Layout::Course { .. } => {
            let end = end::slot(issues, detail, &path, tokens, cx);
            full(&path, end, tokens, cx).into_any_element()
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
            .gap_2()
            .child(body)
            .into_any_element(),
    )
}

fn compact(path: &FlightPath<'_>, tokens: &Tokens, cx: &App) -> AnyElement {
    let state = path.selected();
    h_flex()
        .id("issue-workflow-current")
        .test_support()
        .gap_2()
        .children(state.map(|s| beacon(s, true, tokens, cx)))
        .child(state.map_or_else(|| "Current state unavailable".into(), |s| s.name.clone()))
        .children(
            path.current
                .map(|ix| format!("{} of {}", ix + 1, path.count())),
        )
        .into_any_element()
}

/// The path steps and the end slot share the row equally over one trail.
fn full(path: &FlightPath<'_>, end: Option<AnyElement>, tokens: &Tokens, cx: &App) -> Div {
    h_flex()
        .relative()
        .w_full()
        .min_w_0()
        .items_start()
        .child(trail(path.count(), path.current, *tokens))
        .children(
            path.course
                .iter()
                .map(|state| waypoint(state, path.current_id, tokens, cx)),
        )
        .children(end)
}

/// One stop's share of the row, centred on its point of the trail.
pub(super) fn cell() -> Div {
    v_flex().flex_1().min_w_0().items_center()
}

pub(super) fn waypoint(state: &WorkflowState, current_id: &str, tokens: &Tokens, cx: &App) -> Div {
    let current = state.id == current_id;
    cell()
        .gap_1()
        .child(beacon(state, current, tokens, cx))
        .child(caption(&state.name, current, tokens))
}

pub(super) fn caption(name: &str, current: bool, tokens: &Tokens) -> Div {
    div()
        .text_size(tokens.caption_size())
        .text_center()
        .text_color(if current {
            tokens.fg_strong
        } else {
            tokens.muted
        })
        .child(name.to_owned())
}

pub(super) fn beacon(state: &WorkflowState, current: bool, tokens: &Tokens, cx: &App) -> Div {
    let (glyph, tone) = lanes::state_icon(StateKind::parse(&state.kind));
    ring(current, tokens).child(Icon::from(glyph).small().text_color(mark_colour(
        current,
        tokens.tone(tone),
        cx,
    )))
}

/// The beacon's circle, filled with the accent at the current stop.
pub(super) fn ring(current: bool, tokens: &Tokens) -> Div {
    h_flex()
        .size_8()
        .justify_center()
        .rounded_full()
        .bg(if current { tokens.accent } else { tokens.bg })
        .border_1()
        .border_color(if current { tokens.accent } else { tokens.line })
}

/// The mark inside a ring: on the accent fill at the current stop, else `rest`.
pub(super) fn mark_colour(current: bool, rest: gpui_kit::Hsla, cx: &App) -> gpui_kit::Hsla {
    if current {
        cx.theme().primary_foreground
    } else {
        rest
    }
}

fn trail(count: usize, current: Option<usize>, tokens: Tokens) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, (), window, _| {
            paint_trail(bounds, count, current, tokens, window);
        },
    )
    .absolute()
    .w_full()
    .h_8()
}

fn paint_trail(
    bounds: gpui_kit::Bounds<gpui_kit::Pixels>,
    count: usize,
    current: Option<usize>,
    tokens: Tokens,
    window: &mut Window,
) {
    let width = f32::from(bounds.size.width);
    let rem = f32::from(window.rem_size());
    let centres = super::centres(count, width);
    let start = centres[0];
    let end = width - start;
    let travelled = current.map_or(start, |ix| centres[ix]);
    let origin = bounds.origin + point(px(ZERO), px(COURSE_HEIGHT_REM * rem * HALF));
    if travelled > start {
        window.paint_quad(fill(
            gpui_kit::Bounds::new(
                origin + point(px(start), px(ZERO)),
                size(px(travelled - start), px(HAIRLINE)),
            ),
            tokens.muted,
        ));
    }
    let mut x = travelled + BEACON_REM * rem * HALF;
    while x < end {
        window.paint_quad(fill(
            gpui_kit::Bounds::new(
                origin + point(px(x), px(ZERO)),
                size(px(DOT_REM * rem), px(DOT_REM * rem)),
            ),
            tokens.line,
        ));
        x += DOT_STEP_REM * rem;
    }
}
