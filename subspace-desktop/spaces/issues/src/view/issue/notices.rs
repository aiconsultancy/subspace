use gpui_kit::component::alert::Alert;
use gpui_kit::component::{Sizable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Div, InteractiveElement as _, ParentElement as _, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _,
};
use subspace_api_client::model::IssueDetail;

pub(super) fn render(detail: &IssueDetail) -> Div {
    v_flex()
        .gap_2()
        .when(detail.fields.blocked, |col| {
            col.child(
                gpui_kit::div()
                    .id("issue-blocked")
                    .aria_label("Blocked")
                    .test_support()
                    .child(
                        Alert::warning("issue-blocked-alert", blocked(detail))
                            .title("Blocked")
                            .small(),
                    ),
            )
        })
        .when(detail.fields.archived_at.is_some(), |col| {
            col.child(
                Alert::info(
                    "issue-archived-notice",
                    "This issue is archived. View only.",
                )
                .small(),
            )
        })
}

fn blocked(detail: &IssueDetail) -> String {
    let blockers = detail
        .relations
        .iter()
        .flat_map(|page| &page.items)
        .filter(|relation| relation.kind == "blockedBy")
        .map(|relation| relation.issue.fields.identifier.as_str())
        .collect::<Vec<_>>();
    if blockers.is_empty() {
        "This issue is blocked.".into()
    } else {
        let others = match detail
            .relations
            .as_ref()
            .and_then(|p| p.next_cursor.as_ref())
        {
            Some(_) => " and others",
            None => "",
        };
        format!("Blocked by {}{others}.", blockers.join(", "))
    }
}

#[cfg(test)]
mod tests;
