//! The inspector's status rows: a pull request's reviewers and its checks,
//! each with the glyph that tells its status by shape.

use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, InteractiveElement as _, IntoElement as _, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, div,
};
use subspace_framework::client_github::model::GhItemDetail;
use subspace_framework::dashboard::workspace;
use subspace_framework::dashboard::{Tokens, avatar, links, truncated};

use crate::peek::{self, StatusRow};

/// Reviewers: one row per reviewer with its status, "…" until the details
/// load, or "None".
pub(super) fn reviewers(detail: Option<&GhItemDetail>, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let section = workspace::section("Reviewers", cx);
    let section = match detail {
        None => section.child(quiet("…", &tokens)),
        Some(detail) if detail.reviewers.is_empty() => section.child(quiet("None", &tokens)),
        Some(detail) => section.children(
            peek::reviewer_rows(detail)
                .into_iter()
                .enumerate()
                .map(|(index, row)| status_row("gh-reviewer", index, row, true, &tokens)),
        ),
    };
    section.into_any_element()
}

fn quiet(text: &'static str, tokens: &Tokens) -> Div {
    div()
        .text_size(tokens.secondary_size())
        .text_color(tokens.muted)
        .child(text)
}

/// A reviewer's or a check's row: the status glyph, which tells the status
/// by its shape, an avatar for a person, the name, the status word, and a
/// link to the check's page when it has one.
pub(super) fn status_row(
    prefix: &'static str,
    index: usize,
    row: StatusRow,
    person: bool,
    tokens: &Tokens,
) -> AnyElement {
    let colour = tokens.glyph(&row.glyph);
    let url = row.url.and_then(|url| links::safe_url(&url).map(|_| url));
    let name_id = SharedString::from(format!("{prefix}-name:{index}"));
    h_flex()
        .id((prefix, index))
        .aria_label(format!("{}: {}", row.name, row.word))
        .w_full()
        .min_w_0()
        .gap_2()
        .text_size(tokens.secondary_size())
        .child(
            div()
                .flex_shrink_0()
                .text_color(colour)
                .child(row.glyph.text.clone()),
        )
        .when(person, |line| {
            line.child(avatar::member(row.name.clone(), tokens))
        })
        .child(
            truncated::text(name_id, row.name)
                .flex_1()
                .text_color(tokens.fg_strong),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_color(tokens.muted)
                .child(row.word),
        )
        .when_some(url, |line, url| {
            line.child(
                div()
                    .id(("gh-check-link", index))
                    .aria_label("Open the check on GitHub")
                    .cursor_pointer()
                    .text_color(tokens.accent)
                    .child("↗")
                    .on_click(move |_, _, cx| {
                        links::open(&url, cx);
                    }),
            )
        })
        .into_any_element()
}
