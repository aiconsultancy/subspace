//! The issue's description section, with Edit for those who may change it.

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Context, Div, IntoElement as _, ParentElement as _, Styled as _, div};
use subspace_api_client::model::IssueDetail;
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::rich::rich_text_prepared;
use subspace_framework::lucide::IconName;

use super::{panel, section_heading};
use crate::Issues;

pub(super) fn render(
    issues: &Issues,
    detail: &IssueDetail,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Div {
    div().min_w_0().child(
        panel::section("Description", IconName::FileText, false, tokens)
            .when(issues.can_edit_issue(), |section| {
                section.title(
                    gpui_kit::component::h_flex()
                        .justify_between()
                        .child(section_heading("Description", IconName::FileText, tokens))
                        .child(
                            subspace_framework::dashboard::action::icon_button(
                                "issue-description-edit",
                                subspace_framework::dashboard::action::Intent::Quiet,
                                IconName::Pencil,
                                "Edit description…",
                            )
                            .compact()
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.edit_issue_field(
                                        crate::editing::EditTarget::Description,
                                        window,
                                        cx,
                                    );
                                },
                            )),
                        ),
                )
            })
            .child(match &issues.issue_view.description {
                Some(text) => crate::media::view::markdown(
                    rich_text_prepared(
                        format!("issue-description:{}", detail.fields.id),
                        text.clone(),
                        |url, _, _| {
                            subspace_framework::dashboard::links::safe_url(url).is_none()
                                || url.starts_with("subspace-search:")
                        },
                    ),
                    issues,
                )
                .into_any_element(),
                None => panel::empty("No description", tokens).into_any_element(),
            }),
    )
}
