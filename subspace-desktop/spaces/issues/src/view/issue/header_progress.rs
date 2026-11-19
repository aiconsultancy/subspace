use gpui_kit::component::{h_flex, progress::Progress, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Div, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_api_client::model::{IssueDetail, Progress as TrackerProgress};
use subspace_framework::dashboard::Tokens;

const STRIP_LIMIT: i32 = 16;
const PERCENT: f32 = 100.;

fn percent(done: i32, total: i32) -> f32 {
    done as f32 * PERCENT / total as f32
}

fn progress(detail: &IssueDetail) -> Option<&TrackerProgress> {
    detail
        .fields
        .progress
        .as_ref()
        .filter(|p| detail.fields.kind == "tracker" && p.total > 0)
}

fn counts(p: &TrackerProgress) -> (i32, bool, bool) {
    (
        p.done.clamp(0, p.total),
        p.total <= STRIP_LIMIT,
        p.canceled > 0 || p.duplicate > 0,
    )
}

fn completion(p: &TrackerProgress) -> String {
    format!("{} of {} done", p.done.clamp(0, p.total), p.total)
}

pub(super) fn render(detail: &IssueDetail, tokens: &Tokens) -> Option<AnyElement> {
    let p = progress(detail)?;
    let (done, strip, excluded_visible) = counts(p);
    let text = completion(p);
    let figure = figure(done, p.total, strip, tokens);
    let excluded = format!(
        "Not counted: {} cancelled · {} duplicate",
        p.canceled, p.duplicate
    );
    Some(
        v_flex()
            .id("issue-header-progress")
            .aria_label(format!("{text}. {excluded}"))
            .test_support()
            .gap_1()
            .child(
                h_flex().gap_3().child(figure).child(
                    div()
                        .id("issue-header-progress-text")
                        .aria_label(text.clone())
                        .test_support()
                        .child(text),
                ),
            )
            .when(excluded_visible, |col| {
                col.child(
                    div()
                        .id("issue-header-progress-excluded")
                        .aria_label(excluded.clone())
                        .test_support()
                        .text_color(tokens.muted)
                        .child(excluded),
                )
            })
            .into_any_element(),
    )
}

fn figure(done: i32, total: i32, strip: bool, tokens: &Tokens) -> AnyElement {
    match strip {
        true => div()
            .id("issue-header-progress-marks")
            .test_support()
            .child(marks(done, total, tokens))
            .into_any_element(),
        false => div()
            .id("issue-header-progress-bar")
            .test_support()
            .w_48()
            .child(
                Progress::new("issue-header-progress-native")
                    .color(tokens.accent)
                    .value(percent(done, total)),
            )
            .into_any_element(),
    }
}

#[cfg(test)]
mod tests;

fn marks(done: i32, total: i32, tokens: &Tokens) -> Div {
    h_flex().gap_1().children((0..total).map(|ix| {
        div()
            .w_2()
            .h_2()
            .rounded_full()
            .border_1()
            .border_color(tokens.muted)
            .when(ix < done, |dot| dot.bg(tokens.accent))
    }))
}
