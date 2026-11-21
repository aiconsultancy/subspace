//! The issue's description section, with Edit for those who may change it.
//! The heading is plain semibold text; the Edit pencil stays quiet until the
//! section is under the pointer or the pencil has keyboard focus.

use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Context, Div, InteractiveElement as _, IntoElement as _, ParentElement as _, Styled as _, div,
};
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
    let hover = panel::hover_group("section", "Description");
    let edit = panel::quiet_until_hover(
        subspace_framework::dashboard::action::icon_button(
            "issue-description-edit",
            subspace_framework::dashboard::action::Intent::Quiet,
            IconName::Pencil,
            "Edit description…",
        )
        .compact()
        .on_click(cx.listener(|this, _, window, cx| {
            this.edit_issue_field(crate::editing::EditTarget::Description, window, cx);
        })),
        hover.clone(),
    );
    div().min_w_0().group(hover).child(
        panel::section("Description", IconName::FileText, false, tokens)
            .when(issues.can_edit_issue(), |section| {
                section.title(
                    h_flex()
                        .justify_between()
                        .child(section_heading("Description", IconName::FileText, tokens))
                        .child(edit),
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
