//! The issue's local timeline. The board-wide Activity tab is independent.

mod rows;
mod states;

use gpui_kit::component::group_box::{GroupBox, GroupBoxVariants as _};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Context, Div, InteractiveElement as _, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _,
};
use subspace_api_client::hub::{IssueSnapshot, IssueStatus};
use subspace_framework::dashboard::{
    Tokens,
    action::{ActionState, Intent, action_button},
    tabs::{self, TabSpec},
};
use subspace_framework::lucide::IconName;

use crate::{Issues, detail::timeline::TimelineMode};

pub(super) fn render(
    issues: &Issues,
    issue: &IssueSnapshot,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> impl gpui_kit::IntoElement {
    let timeline = &issues.issue_view.timeline;
    let focus = issues.issue_view.timeline_focus.as_ref();
    // The heading and its Comments/All switch sit above the filled group, as
    // every other section's heading does.
    let panel = v_flex()
        .id("issue-timeline")
        .test_support()
        .anchor_scroll(focus.map(|focus| focus.anchor.clone()))
        .min_w_0()
        .gap_3()
        .child(
            h_flex()
                .justify_between()
                .gap_2()
                .child(super::section_heading(
                    "Activity",
                    IconName::MessagesSquare,
                    tokens,
                ))
                .child(
                    tabs::tab_bar("issue-timeline-tabs")
                        .selected_index(timeline.mode.tab_index())
                        .child(tabs::tab(TabSpec::plain("Comments")))
                        .child(tabs::tab(TabSpec::plain("All")))
                        .on_click(cx.listener(|this, index: &usize, _, cx| {
                            this.show_timeline(TimelineMode::at(*index), cx);
                        })),
                ),
        )
        .when_some(focus, |panel, focus| panel.track_focus(&focus.handle));
    panel.child(
        GroupBox::new().fill().child(
            v_flex()
                .min_w_0()
                .gap_3()
                .children(controls(issues, issue, cx))
                .child(content(issues, issue, tokens, cx))
                .child(crate::authoring::view::root(issues, tokens, cx)),
        ),
    )
}

fn content(
    issues: &Issues,
    issue: &IssueSnapshot,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Div {
    let snapshot = issue.timeline();
    let timeline = &issues.issue_view.timeline;
    let items = snapshot
        .page()
        .map_or(&[][..], |page| page.items.as_slice());
    let mut visible: Vec<_> = timeline.filtered(items).take(timeline.limit()).collect();
    visible.reverse();
    if !visible.is_empty() || !snapshot.projected().is_empty() {
        return rows::render(&visible, timeline.mode, issues, tokens, cx);
    }
    states::empty(snapshot, timeline.mode, issue.status(), tokens)
}
fn controls(issues: &Issues, issue: &IssueSnapshot, cx: &mut Context<Issues>) -> Option<Div> {
    if *issue.status() != IssueStatus::Loaded {
        return None;
    }
    let snapshot = issue.timeline();
    if snapshot.error().is_some() {
        return Some(
            h_flex().child(
                action_button(
                    "issue-timeline-retry",
                    Intent::Neutral,
                    Some(IconName::RotateCw),
                    "Retry",
                )
                .small()
                .on_click(cx.listener(|this, _, _, cx| this.retry_timeline(cx))),
            ),
        );
    }
    let page = snapshot.page()?;
    let view = &issues.issue_view.timeline;
    let hidden = view
        .filtered(&page.items)
        .count()
        .saturating_sub(view.limit());
    if hidden == 0 && page.next_cursor.is_none() {
        return None;
    }
    let label = if snapshot.loading() {
        "Loading earlier entries…".into()
    } else {
        match view.mode {
            TimelineMode::All => "Show earlier activity".into(),
            TimelineMode::Comments if hidden != 0 => {
                let suffix = if page.next_cursor.is_some() { "+" } else { "" };
                format!("Show {hidden}{suffix} earlier comments")
            }
            TimelineMode::Comments => "Show earlier comments".into(),
        }
    };
    let button = action_button(
        "issue-timeline-more",
        Intent::Neutral,
        Some(IconName::ChevronUp),
        label,
    )
    .small()
    .on_click(cx.listener(|this, _, _, cx| this.more_timeline(cx)));
    Some(h_flex().justify_start().child(if snapshot.loading() {
        ActionState::Busy.apply(button)
    } else {
        button
    }))
}
