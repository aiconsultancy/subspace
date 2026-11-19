use super::{
    paging::{self, Rows},
    panel,
};
use crate::{Issues, model::StateKind, view::lanes};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Div, InteractiveElement as _, IntoElement, ParentElement as _,
    Styled as _, TestSupportExt as _, div,
};
use subspace_api_client::hub::{IssueSnapshot, IssueStatus};
use subspace_api_client::model::{IssueDetail, Relation};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::lucide::IconName;

fn kind(value: &str) -> &str {
    match value {
        "blockedBy" => "Blocked by",
        "blocks" => "Blocks",
        "duplicateOf" => "Duplicate of",
        "duplicatedBy" => "Duplicated by",
        "relatedTo" => "Related to",
        _ => "Related",
    }
}

pub(super) fn render(
    detail: &IssueDetail,
    open: &IssueSnapshot,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> AnyElement {
    let page = detail.relations.as_ref();
    let empty = page.is_none_or(|p| p.items.is_empty());
    let rows = v_flex()
        .gap_1()
        .when(empty, |col| col.child(panel::empty("No relations", tokens)))
        .children(
            page.into_iter()
                .flat_map(|p| &p.items)
                .map(|item| row(item, tokens, cx)),
        )
        .when(
            *open.status() == IssueStatus::Loaded && page.is_some_and(|p| p.next_cursor.is_some()),
            |col| col.child(paging::render(Rows::Relations, open, tokens, cx)),
        );
    div()
        .id("issue-relations")
        .test_support()
        .child(panel::section("Relations", IconName::Link, !empty, tokens).child(rows))
        .into_any_element()
}

fn row(item: &Relation, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    let fields = &item.issue.fields;
    let identifier = fields.identifier.clone();
    let direction = kind(&item.kind);
    let (icon, _) = lanes::state_icon(StateKind::parse(&fields.state.kind));
    let button = action_button(
        format!("issue-relation:{}:{}", item.kind, fields.id),
        Intent::Quiet,
        Some(icon),
        format!("{identifier} · {}", fields.title),
    )
    .accessibility_label(format!(
        "{direction}: {identifier}, {}, {}",
        fields.state.name, fields.title
    ))
    .on_click(cx.listener(move |this, _, window, cx| {
        this.focus_list(window, cx);
        this.open_detail(identifier.clone(), cx);
    }));
    h_flex()
        .gap_2()
        .border_b_1()
        .border_color(tokens.hair)
        .child(
            div()
                .w_24()
                .flex_shrink_0()
                .text_color(tokens.muted)
                .child(direction.to_owned()),
        )
        .child(
            h_flex()
                .min_w_0()
                .flex_1()
                .justify_start()
                .child(button.max_w_full()),
        )
}
