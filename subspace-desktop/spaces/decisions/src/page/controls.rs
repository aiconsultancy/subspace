//! Actions that apply to the currently displayed decision.

use gpui_kit::component::h_flex;
use gpui_kit::{AnyElement, Context, IntoElement as _, ParentElement as _, Styled as _};
use subspace_api_client::decision_writes::DecisionReview;
use subspace_api_client::model::DecisionDetail;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::lucide::IconName;

use crate::Decisions;

pub(super) fn render(
    board: &Decisions,
    detail: &DecisionDetail,
    cx: &mut Context<Decisions>,
) -> AnyElement {
    let mut row = h_flex().flex_wrap().gap_2();
    let current = board.history.snapshot().is_none();
    if current && board.can_change(detail) {
        row = row.children(review_controls(cx));
    }
    if current && board.can_replace(detail) {
        row = row.child(
            action_button(
                "decision-replace",
                Intent::Neutral,
                Some(IconName::Plus),
                "Propose replacement",
            )
            .on_click(cx.listener(|this, _, window, cx| this.new_decision(true, window, cx))),
        );
    }
    let label = if board.history.open {
        "Hide revision history"
    } else {
        "Revision history"
    };
    row.child(
        action_button(
            "decision-history",
            Intent::Neutral,
            Some(IconName::Clock),
            label,
        )
        .on_click(cx.listener(|this, _, _, cx| this.toggle_history(cx))),
    )
    .into_any_element()
}

fn review_controls(cx: &mut Context<Decisions>) -> [AnyElement; 3] {
    [
        action_button(
            "decision-edit",
            Intent::Neutral,
            Some(IconName::Pencil),
            "Edit",
        )
        .on_click(cx.listener(|this, _, window, cx| this.edit_decision(window, cx)))
        .into_any_element(),
        action_button(
            "decision-accept",
            Intent::Accept,
            Some(IconName::Check),
            "Accept",
        )
        .on_click(cx.listener(|this, _, window, cx| {
            this.review_decision(DecisionReview::Accept, window, cx)
        }))
        .into_any_element(),
        action_button(
            "decision-reject",
            Intent::Neutral,
            Some(IconName::X),
            "Reject",
        )
        .on_click(cx.listener(|this, _, window, cx| {
            this.review_decision(DecisionReview::Reject, window, cx)
        }))
        .into_any_element(),
    ]
}
