//! The decision detail toolbar and the actions for the visible revision.

use gpui_kit::component::h_flex;
use gpui_kit::{Context, ParentElement as _, SharedString, Styled as _};
use subspace_api_client::decision_writes::DecisionReview;
use subspace_framework::dashboard::action::{Intent, action_button, icon_button, main_action};
use subspace_framework::dashboard::icon_button::IconButton;
use subspace_framework::dashboard::workspace::Toolbar;
use subspace_framework::dashboard::{ReturnPoint, Tokens, truncated};
use subspace_framework::lucide::IconName;

use super::{Page, PageState};
use crate::Decisions;

pub(crate) fn toolbar(board: &Decisions, page: &Page, cx: &mut Context<Decisions>) -> Toolbar {
    let label = page.origin.as_ref().map_or_else(
        || SharedString::from("Back to decisions"),
        ReturnPoint::back_label,
    );
    let back = icon_button(
        "decisions-page-back",
        Intent::Quiet,
        IconName::ArrowLeft,
        label,
    )
    .on_click(cx.listener(|this, _, window, cx| this.go_back(window, cx)));
    let bar = Toolbar::new("decisions-page-toolbar");
    let PageState::Loaded(detail) = &page.state else {
        return bar.leading(back).trailing(crate::toolbar::refresh(cx));
    };
    let path = h_flex()
        .min_w_0()
        .max_w_full()
        .gap_2()
        .child(back.flex_shrink_0())
        .children(detail.identifier.as_deref().map(|id| reference(id, cx)));
    let bar = bar.leading(path);
    detail_actions(board, detail, bar, cx)
}

fn detail_actions(
    board: &Decisions,
    detail: &subspace_api_client::model::DecisionDetail,
    bar: Toolbar,
    cx: &Context<Decisions>,
) -> Toolbar {
    use gpui_kit::prelude::FluentBuilder as _;
    let can_author = crate::author::available();
    let review = board.review_item().is_some();
    let replace = board.history.snapshot().is_none() && board.can_replace(detail);
    bar.when(can_author, |bar| {
        bar.trailing(history_button(board.history.open, cx))
    })
    .when(review, |bar| review_controls(bar, cx))
    .when(!review, |bar| bar.trailing(crate::toolbar::refresh(cx)))
    .when(replace, |bar| bar.trailing(replacement_button(cx)))
}

fn reference(identifier: &str, cx: &Context<Decisions>) -> impl gpui_kit::IntoElement {
    let tokens = Tokens::current(cx);
    truncated::text("decisions-path-identifier", identifier.to_owned())
        .text_size(tokens.secondary_size())
        .text_color(tokens.fg_strong)
}

fn history_button(open: bool, cx: &Context<Decisions>) -> IconButton {
    let label = if open {
        "Hide revision history"
    } else {
        "Revision history"
    };
    icon_button("decision-history", Intent::Quiet, IconName::Clock, label)
        .latched(open)
        .on_click(cx.listener(|this, _, _, cx| this.toggle_history(cx)))
}

fn replacement_button(cx: &Context<Decisions>) -> IconButton {
    action_button(
        "decision-replace",
        Intent::Neutral,
        Some(IconName::Plus),
        "Propose replacement",
    )
    .on_click(cx.listener(|this, _, window, cx| this.new_decision(true, window, cx)))
}

fn review_controls(bar: Toolbar, cx: &Context<Decisions>) -> Toolbar {
    let edit = icon_button("decision-edit", Intent::Quiet, IconName::Pencil, "Edit")
        .on_click(cx.listener(|this, _, window, cx| this.edit_decision(window, cx)));
    let reject = action_button(
        "decision-reject",
        Intent::Neutral,
        Some(IconName::X),
        "Reject",
    )
    .on_click(cx.listener(|this, _, window, cx| {
        this.review_decision(DecisionReview::Reject, window, cx);
    }));
    let accept = main_action("decision-accept", IconName::Check, "Accept").on_click(cx.listener(
        |this, _, window, cx| {
            this.review_decision(DecisionReview::Accept, window, cx);
        },
    ));
    bar.trailing(edit)
        .trailing(reject)
        .trailing(crate::toolbar::refresh(cx))
        .trailing(accept)
}
