//! The issue opens with its concise summary, using the retained inline draft.
use super::{panel, section_heading};
use crate::{
    Issues,
    editing::{self, EditTarget},
};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Context, Div, InteractiveElement as _, IntoElement as _, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_api_client::model::IssueDetail;
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, icon_button},
    rich,
};
use subspace_framework::lucide::IconName;

pub(super) fn section(
    board: &Issues,
    detail: &IssueDetail,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Div {
    let heading = h_flex()
        .justify_between()
        .child(section_heading("Summary", IconName::FileText, tokens))
        .when(
            board.can_edit_issue() && !board.inline_field_active(),
            |row| {
                row.child(
                    icon_button(
                        "issue-summary-edit",
                        Intent::Quiet,
                        IconName::Pencil,
                        "Edit summary",
                    )
                    .compact()
                    .on_click(cx.listener(|board, _, window, cx| {
                        board.edit_issue_field(EditTarget::Summary, window, cx);
                    })),
                )
            },
        );
    let content = editing::view::inline_row(board, "Summary", tokens, cx).unwrap_or_else(|| {
        match detail.fields.summary.as_deref() {
            Some(text) => rich::plain_text(format!("issue-summary:{}", detail.fields.id), text)
                .into_any_element(),
            None => panel::empty("No summary yet", tokens).into_any_element(),
        }
    });
    v_flex().min_w_0().child(
        panel::section("Summary", IconName::FileText, false, tokens)
            .title(heading)
            .child(
                div()
                    .id("issue-summary-content")
                    .test_support()
                    .aria_label(
                        detail
                            .fields
                            .summary
                            .as_deref()
                            .unwrap_or("No summary yet")
                            .to_owned(),
                    )
                    .child(content),
            ),
    )
}
