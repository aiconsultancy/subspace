//! The board as Subspace API's Issues page lays it out: the heading and scope line,
//! the four tabs, the toolbar, the summary, then the grouped list or a
//! state in its place (design `list`, `list-loading`, `list-signed-out`).

mod issue;
mod render;
pub(crate) use issue::section_heading;
pub(crate) mod lanes;
pub(crate) mod progress;
mod states;

use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Div, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
    px,
};
use subspace_api_client::hub::LiveStatus;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::fields::{self, InputRole};
use subspace_framework::dashboard::tabs::{self, TabSpec, TabStatus};
use subspace_framework::dashboard::{ListDashboard as _, Tokens, feedback, list};
use subspace_framework::lucide::IconName;

use crate::filter_menu::{self, FilterMenu};
use crate::listing::Status;
use crate::model::{SubIssues, Tab};
use crate::scope::{self, Body};
use crate::{Issues, initiative, keys, pickers};

/// The approved Subspace API layout's shared content edge.
pub(crate) const SIDE_INSET_PX: f32 = 24.;

/// The search field's width, as Subspace API's toolbar has it.
const SEARCH_WIDTH_PX: f32 = 260.;
/// Issues and the connection state.
fn header(issues: &Issues, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    list::header(
        "issues",
        "Issues",
        scope::scope_line(issues.profile_name(), &issues.access),
        issues.list_side_inset().unwrap_or(tokens.pad_x),
        cx,
    )
    .when_some(
        scope::live_line(&issues.access, issues.live),
        |header, live| header.child(live_indicator(live, issues.live, tokens)),
    )
}

/// The live indicator's dot, small beside secondary text.
const LIVE_DOT_PX: f32 = 6.0;

/// A small dot and word for how changes elsewhere arrive, under the scope.
fn live_indicator(words: &'static str, live: LiveStatus, tokens: &Tokens) -> impl IntoElement {
    let colour = match live {
        LiveStatus::Live => tokens.green,
        LiveStatus::Reconnecting | LiveStatus::Connecting => tokens.yellow,
        LiveStatus::Polling => tokens.muted,
    };
    h_flex()
        .id("issues-live")
        .aria_label(words)
        .test_support()
        .gap_1()
        .text_size(tokens.secondary_size())
        .text_color(tokens.muted)
        .child(div().size(px(LIVE_DOT_PX)).rounded_full().bg(colour))
        .child(words)
}

/// The tabs, the toolbar and the summary line.
fn controls(issues: &Issues, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    v_flex()
        .flex_shrink_0()
        .px(px(SIDE_INSET_PX))
        .gap_2()
        .child(tab_bar(issues, cx))
        .child(toolbar(issues, cx))
        .child(summary(issues, tokens, cx))
}

fn activity_controls(issues: &Issues, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    v_flex()
        .flex_shrink_0()
        .px(tokens.pad_x)
        .pb_2()
        .gap_2()
        .child(tab_bar(issues, cx))
        .child(
            h_flex()
                .gap_2()
                .children(initiative::button(issues, cx))
                .child(pickers::project_button(issues, cx))
                .when(issues.can_create_issue(), |row| {
                    row.child(crate::editing::view::new_button(cx))
                }),
        )
}

/// The selected tab's position; Activity follows the issue tabs.
fn selected_tab_index(issues: &Issues) -> usize {
    if issues.activity.visible {
        Tab::ALL.len()
    } else {
        issues.tab.index()
    }
}

fn tab_bar(issues: &Issues, cx: &mut Context<Issues>) -> impl IntoElement {
    let bar = tabs::tab_bar("issues-tab-bar")
        .selected_index(selected_tab_index(issues))
        .children(
            Tab::ALL
                .into_iter()
                .map(|tab| {
                    let spec = tab.spec();
                    let mut tab_spec = TabSpec::new(spec.label, spec.icon);
                    if let (Tab::MyIssues, Some(count)) = (tab, issues.listing.my_issues_count) {
                        let noun = if count == 1 { "issue" } else { "issues" };
                        tab_spec = tab_spec.status(TabStatus::text(
                            count.to_string(),
                            format!("{count} open {noun}"),
                            Tokens::current(cx).muted,
                        ));
                    }
                    tabs::tab(tab_spec)
                })
                .chain(std::iter::once(tabs::tab(TabSpec::new(
                    "Activity",
                    IconName::Activity,
                )))),
        )
        .on_click(cx.listener(|this, index: &usize, window, cx| {
            if let Some(tab) = Tab::ALL.get(*index) {
                this.set_tab(*tab, cx);
            } else if *index == Tab::ALL.len() {
                this.open_activity(window, cx);
            }
        }));
    div().id("issues-tabs").test_support().child(bar)
}

/// Search and project on the left, the Sub-issues switch on the right.
fn toolbar(issues: &Issues, cx: &mut Context<Issues>) -> impl IntoElement {
    let shown = issues.sub_issues == SubIssues::Flat;
    let search = fields::input(&issues.search, InputRole::Search, "Search issues")
        .prefix(Icon::from(IconName::Search).xsmall())
        .cleanable(true);
    h_flex()
        .id("issues-toolbar")
        .test_support()
        .w_full()
        .flex_wrap()
        .gap_2()
        .child(
            div()
                .key_context(keys::SEARCH_CONTEXT)
                .w(px(SEARCH_WIDTH_PX))
                .child(search.id("issues-search")),
        )
        .children(initiative::button(issues, cx))
        .child(pickers::project_button(issues, cx))
        .child(filter_menu::button(FilterMenu::Assignee, issues, cx))
        .child(filter_menu::button(FilterMenu::Priority, issues, cx))
        .child(filter_menu::button(FilterMenu::More, issues, cx))
        .child(div().flex_1())
        .when(issues.can_create_issue(), |row| {
            row.child(crate::editing::view::new_button(cx))
        })
        .child(
            action_button(
                "issues-sub-issues",
                Intent::Neutral,
                Some(IconName::ListTree),
                "Sub-issues",
            )
            .latched(shown)
            .on_click(cx.listener(|this, _, _, cx| this.toggle_sub_issues(cx))),
        )
}

/// *n issues*, or *Loading issues…* while the first page reads.
fn summary(issues: &Issues, tokens: &Tokens, cx: &mut Context<Issues>) -> AnyElement {
    let mut text: SharedString = match issues.listing.issue_count() {
        _ if crate::initiative::no_initiative_projects(issues) => "".into(),
        Some(1) => "1 issue".into(),
        Some(count) => format!("{count} issues").into(),
        None if matches!(issues.listing.status, Status::Failed(_)) => "".into(),
        None => "Loading issues…".into(),
    };
    let hidden = issues.sub_issues == SubIssues::Hidden
        && issues.listing.content.is_some()
        && issues
            .listing
            .hidden_subissues_count
            .is_some_and(|count| count > 0);
    if hidden {
        let count = issues.listing.hidden_subissues_count.unwrap_or_default();
        text = format!("{text} · {count} sub-issues hidden").into();
    }
    h_flex()
        .id("issues-summary")
        .gap_2()
        .aria_label(text.clone())
        .test_support()
        .pb_1()
        .text_size(tokens.secondary_size())
        .text_color(tokens.muted)
        .child(text)
        .when(hidden, |row| {
            row.child(
                action_button(
                    "issues-show-sub-issues",
                    Intent::Neutral,
                    None,
                    "Show sub-issues",
                )
                .on_click(cx.listener(|this, _, _, cx| this.toggle_sub_issues(cx))),
            )
        })
        .into_any_element()
}

fn body_element(
    issues: &Issues,
    body: &Body,
    tokens: &Tokens,
    window: &mut Window,
    cx: &mut Context<Issues>,
) -> AnyElement {
    let frame = div().flex_1().min_h_0().w_full();
    match body {
        Body::List => frame
            .child(list::render(issues, window, cx))
            .into_any_element(),
        Body::Loading { .. } => frame
            .child(
                div()
                    .id("issues-loading")
                    .aria_label("Loading issues")
                    .test_support()
                    .child(feedback::skeleton(tokens))
                    .child("The first load can take up to a minute while Subspace API wakes."),
            )
            .into_any_element(),
        Body::Screen { screen, .. } => frame
            .child(states::render(issues, screen, tokens, cx))
            .into_any_element(),
    }
}

/// Subspace API's empty wording for the current filters, search or tab.
pub(crate) fn empty_message(tab: Tab, query: &str, filtered: bool) -> (SharedString, SharedString) {
    if filtered {
        return (
            "No issues match your filters".into(),
            "Try changing your filters.".into(),
        );
    }
    if !query.is_empty() {
        return (
            "No issues match your search".into(),
            "Try other words, or clear the search.".into(),
        );
    }
    (
        tab.spec().empty.into(),
        "Try another project, or reload.".into(),
    )
}
