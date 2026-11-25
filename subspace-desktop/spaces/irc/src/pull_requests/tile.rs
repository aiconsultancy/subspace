//! A pull request's passive hover card, in the issue card's shared
//! `reference_card` outline treatment: the `repo#n` key and state
//! pill, the title, branches and changed lines when known, then author,
//! review and check marks and how long ago it changed. Each mark pairs its
//! glyph with words, so colour never carries meaning alone.

use chrono::{DateTime, Utc};
use gpui_kit::base::TestTextExt as _;
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{App, Div, ParentElement as _, Styled as _, div, rems};
use subspace_framework::contracts::github::{
    CheckStatus, PullRequestDetails, PullRequestStatus, ReviewStatus,
};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::reference_card::{self, Treatment};
use subspace_framework::dashboard::{Tokens, Tone};

/// The hover card's width: 320 px at the reference text size.
pub(crate) const CARD_WIDTH_REM: f32 = 320. / subspace_framework::dashboard::density::REFERENCE_REM;
const TITLE_REM: f32 = 14. / subspace_framework::dashboard::density::REFERENCE_REM;
const TITLE_LINES: usize = 2;

/// A mark's glyph, tone and words.
pub(super) type Mark = (IconName, Tone, &'static str);

pub(super) fn status(status: PullRequestStatus) -> Mark {
    match status {
        PullRequestStatus::Draft => (IconName::GitPullRequestDraft, Tone::Muted, "Draft"),
        PullRequestStatus::Merged => (IconName::GitMerge, Tone::Magenta, "Merged"),
        PullRequestStatus::Closed => (IconName::GitPullRequestClosed, Tone::Red, "Closed"),
        _ => (IconName::GitPullRequest, Tone::Green, "Open"),
    }
}

pub(super) fn review(review: ReviewStatus) -> Mark {
    match review {
        ReviewStatus::Approved => (IconName::CircleCheck, Tone::Green, "Approved"),
        ReviewStatus::ChangesRequested => (IconName::CircleX, Tone::Red, "Changes requested"),
        _ => (IconName::Eye, Tone::Muted, "Review required"),
    }
}

pub(super) fn checks(checks: CheckStatus) -> Mark {
    match checks {
        CheckStatus::Success => (IconName::Check, Tone::Green, "Checks passed"),
        CheckStatus::Failure => (IconName::X, Tone::Red, "Checks failed"),
        _ => (IconName::CircleDot, Tone::Yellow, "Checks running"),
    }
}

/// `head → base`, when both are known.
pub(super) fn branches(details: &PullRequestDetails) -> Option<String> {
    let head = details.head_ref.as_deref()?;
    let base = details.base_ref.as_deref()?;
    Some(format!("{head} → {base}"))
}

/// Lines added and deleted, as `+12` and `−3`, when known.
pub(super) fn changes(details: &PullRequestDetails) -> Option<(String, String)> {
    Some((
        format!("+{}", details.additions?),
        format!("−{}", details.deletions?),
    ))
}

pub(super) fn hover_card(
    details: &PullRequestDetails,
    label: &str,
    now: DateTime<Utc>,
    tokens: &Tokens,
    cx: &App,
) -> Div {
    let content = v_flex()
        .flex_1()
        .min_w_0()
        .gap_1()
        .child(header(details, label, tokens, cx))
        .child(title(details, tokens))
        .when_some(branch_line(details, tokens, cx), |card, line| {
            card.child(line)
        })
        .child(footer(details, now, tokens));
    reference_card::frame(content, Treatment::Outline, tokens, cx).w(rems(CARD_WIDTH_REM))
}

fn header(details: &PullRequestDetails, label: &str, tokens: &Tokens, cx: &App) -> Div {
    let (glyph, tone, words) = status(details.status);
    let owner = details
        .key
        .split_once('/')
        .map(|(owner, _)| owner.to_owned());
    h_flex()
        .justify_between()
        .min_w_0()
        .gap_2()
        .child(
            h_flex()
                .min_w_0()
                .gap_1p5()
                .child(
                    div()
                        .flex_shrink_0()
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_size(tokens.secondary_size())
                        .text_color(tokens.fg)
                        .child(label.to_owned().test_text(label.to_owned())),
                )
                .when_some(owner, |row, owner| row.child(muted_text(owner, tokens))),
        )
        .child(reference_card::pill(words, glyph, tone, tokens))
}

fn title(details: &PullRequestDetails, tokens: &Tokens) -> Div {
    div()
        .min_w_0()
        .whitespace_normal()
        .line_clamp(TITLE_LINES)
        .font_weight(gpui_kit::FontWeight::MEDIUM)
        .text_size(rems(TITLE_REM))
        .text_color(tokens.fg_strong)
        .child(details.title.clone().test_text(details.title.clone()))
}

/// The branches and changed lines, when either is known.
fn branch_line(details: &PullRequestDetails, tokens: &Tokens, cx: &App) -> Option<Div> {
    let branches = branches(details);
    let changes = changes(details);
    if branches.is_none() && changes.is_none() {
        return None;
    }
    Some(
        h_flex()
            .min_w_0()
            .gap_2()
            .text_size(tokens.secondary_size())
            .text_color(tokens.muted)
            .when_some(branches, |row, branches| {
                row.child(Icon::from(IconName::GitBranch).size(tokens.secondary_size()))
                    .child(
                        div()
                            .min_w_0()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .font_family(cx.theme().mono_font_family.clone())
                            .child(branches.clone().test_text(branches)),
                    )
            })
            .when_some(changes, |row, (added, deleted)| {
                row.child(
                    h_flex()
                        .flex_shrink_0()
                        .gap_1()
                        .child(
                            div()
                                .text_color(tokens.green)
                                .child(added.clone().test_text(added)),
                        )
                        .child(
                            div()
                                .text_color(tokens.red)
                                .child(deleted.clone().test_text(deleted)),
                        ),
                )
            }),
    )
}

fn footer(details: &PullRequestDetails, now: DateTime<Utc>, tokens: &Tokens) -> Div {
    let updated = details
        .updated_at
        .as_deref()
        .and_then(|at| crate::issues::relative(at, now))
        .map(|relative| relative.label);
    h_flex()
        .min_w_0()
        .flex_wrap()
        .gap_2()
        .mt_1()
        .text_size(tokens.secondary_size())
        .text_color(tokens.fg)
        .when_some(details.author.clone(), |row, author| {
            row.child(reference_card::avatar(&author, false, true, tokens))
                .child(
                    div()
                        .min_w_0()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(author),
                )
        })
        .when_some(details.review.map(review), |row, mark| {
            row.child(pill(mark, tokens))
        })
        .when_some(details.checks.map(checks), |row, mark| {
            row.child(pill(mark, tokens))
        })
        .when_some(updated, |row, updated| {
            row.child(muted_text(format!("Updated {updated}"), tokens))
        })
}

fn pill((glyph, tone, words): Mark, tokens: &Tokens) -> Div {
    reference_card::pill(words, glyph, tone, tokens)
}

fn muted_text(text: String, tokens: &Tokens) -> Div {
    div()
        .min_w_0()
        .text_ellipsis()
        .whitespace_nowrap()
        .text_color(tokens.muted)
        .child(text.clone().test_text(text))
}
