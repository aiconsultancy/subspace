use super::{activity, facts};
use chrono::{DateTime, Utc};
use gpui_kit::base::TestTextExt as _;
use gpui_kit::component::{Icon, Sizable as _, h_flex, progress::Progress, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{App, Div, ParentElement as _, SharedString, Styled as _};
use subspace_framework::contracts::issues::IssueSummary;
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, Tone};
const PERCENT: f32 = 100.;

pub(super) fn card(summary: &IssueSummary, now: DateTime<Utc>, tokens: &Tokens, cx: &App) -> Div {
    v_flex()
        .flex_1()
        .min_w_0()
        .gap_1()
        .child(
            h_flex()
                .justify_between()
                .gap_2()
                .min_w_0()
                .child(super::key(
                    format!("{} · Tracker", summary.identifier),
                    tokens,
                    cx,
                ))
                .child(super::state(summary, tokens)),
        )
        .child(super::title(summary, tokens))
        .children(super::summary_text(summary, tokens))
        .when(summary.archived, |card| card.child("Archived"))
        .when_some(
            summary.progress().filter(|progress| progress.total > 0),
            |card, progress| {
                let label = format!("{} of {} done", progress.done, progress.total);
                card.child(
                    h_flex()
                        .min_w_0()
                        .gap_2()
                        .mt_1()
                        .text_size(tokens.secondary_size())
                        .text_color(tokens.fg)
                        .child(
                            Progress::new(SharedString::from(format!(
                                "issue-progress:{}",
                                summary.identifier
                            )))
                            .xsmall()
                            .flex_1()
                            .min_w_0()
                            .bg(tokens.line)
                            .color(tokens.green)
                            .value(progress.done as f32 / progress.total as f32 * PERCENT)
                            .accessibility_label(label.clone()),
                        )
                        .child(label.clone().test_text(label)),
                )
            },
        )
        .child(metadata(summary, now, tokens))
}
fn metadata(summary: &IssueSummary, now: DateTime<Utc>, tokens: &Tokens) -> Div {
    h_flex()
        .min_w_0()
        .flex_wrap()
        .gap_x_3()
        .gap_y_1()
        .text_size(tokens.secondary_size())
        .when_some(summary.pull_requests(), |row, prs| {
            row.when_some(facts::open_prs(prs), |row, (label, tone)| {
                row.child(fact(label, IconName::GitPullRequest, tone, tokens))
            })
            .when(prs.merged > 0, |row| {
                row.child(fact(
                    format!("{} merged", prs.merged),
                    IconName::GitMerge,
                    Tone::Magenta,
                    tokens,
                ))
            })
        })
        .when_some(
            summary
                .due_date()
                .and_then(|due| activity::due(due, now.date_naive())),
            |row, label| row.child(fact(label, IconName::Calendar, Tone::Muted, tokens)),
        )
}
fn fact(
    label: String,
    glyph: subspace_framework::lucide::SvgIcon,
    tone: Tone,
    tokens: &Tokens,
) -> Div {
    h_flex()
        .min_w_0()
        .mt_1()
        .gap_1()
        .text_color(tokens.tone(tone))
        .child(Icon::from(glyph).size(tokens.secondary_size()))
        .child(label)
}
