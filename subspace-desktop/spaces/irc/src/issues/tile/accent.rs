use super::{activity, facts};
use chrono::{DateTime, Utc};
use gpui_kit::base::TestTextExt as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{App, Div, ParentElement as _, Styled as _, div};
use subspace_framework::contracts::issues::IssueSummary;
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, Tone, reference_card};

pub(super) fn card(summary: &IssueSummary, now: DateTime<Utc>, tokens: &Tokens, cx: &App) -> Div {
    v_flex()
        .flex_1()
        .min_w_0()
        .gap_1()
        .child(header(summary, tokens, cx))
        .child(super::title(summary, tokens))
        .children(super::summary_text(summary, tokens))
        .when(facts::has_footer(summary), |card| {
            card.child(footer(summary, now, tokens))
        })
}
fn header(summary: &IssueSummary, tokens: &Tokens, cx: &App) -> Div {
    h_flex()
        .justify_between()
        .min_w_0()
        .gap_2()
        .child(
            h_flex()
                .min_w_0()
                .gap_2()
                .child(super::key(summary.identifier.clone(), tokens, cx))
                .when_some(facts::priority(summary.priority()), |row, label| {
                    row.child(reference_card::pill(
                        label,
                        IconName::ChevronsUp,
                        Tone::Red,
                        tokens,
                    ))
                })
                .when(summary.archived, |row| row.child("Archived")),
        )
        .child(super::state(summary, tokens))
}
fn footer(summary: &IssueSummary, now: DateTime<Utc>, tokens: &Tokens) -> Div {
    h_flex()
        .min_w_0()
        .justify_between()
        .gap_2()
        .mt_1()
        .text_size(tokens.secondary_size())
        .text_color(tokens.fg)
        .child(assignment(summary, tokens))
        .when_some(
            summary
                .latest_activity()
                .and_then(|last| activity::relative(&last.occurred_at, now)),
            |row, activity| {
                row.child(
                    h_flex()
                        .flex_shrink_0()
                        .gap_1()
                        .text_color(tokens.muted)
                        .child(
                            div()
                                .text_color(if activity.recent {
                                    tokens.green
                                } else {
                                    tokens.muted
                                })
                                .child("•"),
                        )
                        .child(activity.label.clone().test_text(activity.label)),
                )
            },
        )
}
fn assignment(summary: &IssueSummary, tokens: &Tokens) -> Div {
    h_flex()
        .min_w_0()
        .gap_2()
        .when_some(summary.assignee.as_deref(), |row, name| {
            row.child(reference_card::avatar(
                name,
                facts::is_agent(summary),
                false,
                tokens,
            ))
            .child(
                div()
                    .min_w_0()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(name.to_owned()),
            )
            .when_some(facts::rank(summary), |row, rank| {
                row.child(div().text_color(tokens.muted).child(format!("· {rank}")))
            })
        })
        .when_some(facts::owner(summary), |row, owner| {
            row.child(
                div()
                    .min_w_0()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_color(tokens.muted)
                    .child(format!("for {owner}")),
            )
        })
}
