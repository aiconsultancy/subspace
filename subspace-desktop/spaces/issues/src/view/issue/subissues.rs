use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::{Context, Div, ParentElement as _, Styled as _, div};
use subspace_api_client::hub::{IssueSnapshot, IssueStatus};
use subspace_api_client::model::{IssueDetail, IssueFields};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{ActionState, Intent, action_button};
use subspace_framework::lucide::IconName;

use crate::Issues;
use crate::model::StateKind;
use crate::view::lanes;

pub(super) fn render(
    issues: &Issues,
    detail: &IssueDetail,
    open: &IssueSnapshot,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Option<Div> {
    let page = detail.children.as_ref();
    let rows = page
        .into_iter()
        .flat_map(|page| page.items.iter().map(|item| &item.fields));
    let mut created = issues.issue_view.projected_subissues().rev().peekable();
    let has_more = page.is_some_and(|page| page.next_cursor.is_some());
    let has_rows =
        has_more || created.peek().is_some() || page.is_some_and(|page| !page.items.is_empty());
    let can_create = issues.can_create_subissue(detail);
    if !has_rows && !can_create && detail.fields.kind != "tracker" {
        return None;
    }
    let more = (*open.status() == IssueStatus::Loaded && has_more)
        .then(|| more_button(detail, open, tokens, cx));
    Some(
        div().min_w_0().child(
            super::panel::section("Sub-issues", IconName::ListTree, has_rows, tokens).child(
                v_flex()
                    .min_w_0()
                    .gap_1()
                    .children(can_create.then(|| new_button(cx)))
                    .children((!has_rows).then(|| super::panel::empty("No sub-issues yet", tokens)))
                    .children(created.chain(rows).map(|fields| row(fields, tokens, cx)))
                    .children(more),
            ),
        ),
    )
}

fn new_button(cx: &mut Context<Issues>) -> Div {
    h_flex().child(
        action_button(
            "issue-new-subissue",
            Intent::Neutral,
            Some(IconName::Plus),
            "New sub-issue",
        )
        .small()
        .on_click(cx.listener(|this, _, window, cx| this.new_subissue(window, cx))),
    )
}

fn row(fields: &IssueFields, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    let identifier = fields.identifier.clone();
    let (icon, tone) = lanes::state_icon(StateKind::parse(&fields.state.kind));
    let button = action_button(
        format!("issue-child:{}", fields.id),
        Intent::Quiet,
        Some(icon),
        format!("{identifier} · {}", fields.title),
    )
    .small()
    .max_w_full()
    .accessibility_label(format!(
        "{identifier}, {}, {}",
        fields.state.name, fields.title
    ))
    .on_click(cx.listener(move |this, _, window, cx| {
        this.focus_list(window, cx);
        this.open_detail(identifier.clone(), cx);
    }));
    h_flex()
        .min_w_0()
        .justify_between()
        .gap_2()
        .border_b_1()
        .border_color(tokens.hair)
        .child(h_flex().flex_1().min_w_0().justify_start().child(button))
        .child(
            div()
                .flex_shrink_0()
                .text_color(tokens.tone(tone))
                .child(fields.state.name.clone()),
        )
}

fn more_button(
    detail: &IssueDetail,
    open: &IssueSnapshot,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Div {
    let remaining = detail.fields.progress.as_ref().and_then(|progress| {
        let shown = detail.children.as_ref().map_or(0, |page| page.items.len());
        usize::try_from(progress.total)
            .ok()?
            .checked_sub(shown)
            .filter(|n| *n > 0)
    });
    let label = super::paging::label(open.children_loading(), open.children_failed(), remaining);
    let icon = match (open.children_loading(), open.children_failed()) {
        (true, _) => IconName::Ellipsis,
        (_, true) => IconName::RotateCw,
        _ => IconName::ChevronDown,
    };
    let button = action_button("issue-more-subissues", Intent::Neutral, Some(icon), label)
        .on_click(cx.listener(|this, _, window, cx| {
            this.focus_list(window, cx);
            this.more_subissues(cx);
        }));
    let button = if open.children_loading() {
        ActionState::Busy.apply(button)
    } else {
        button
    };
    h_flex()
        .gap_2()
        .children(open.children_failed().then(|| {
            div()
                .text_color(tokens.muted)
                .child("Couldn't load more sub-issues")
        }))
        .child(button)
}
