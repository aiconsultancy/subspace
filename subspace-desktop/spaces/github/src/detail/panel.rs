//! Everything under the detail panel's properties: reviewers, the action bar or the composer, checks
//! (collapsed above five), the description and the activity.

use gpui_kit::component::{Disableable as _, Selectable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, ElementId, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, Window, div, px,
};
use subspace_framework::client_github::model::{GhItem, GhItemDetail, ItemKind, display_name};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::anchored::{self, Anchor};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::rich::{self, Format, rich_text};
use subspace_framework::dashboard::truncated;
use subspace_framework::dashboard::{Glyph, Tokens, feedback, links};

use super::Mode;
use super::actions::{ActionSpec, footer_actions};

mod activity;
mod page;

pub use page::page_sections;

use crate::model::{CHECKS_SHOWN, can_comment, can_review, check_summary, is_failed_check};
use crate::rows::{check_glyph, reviewer_badge};
use activity::activity;

/// The space above a heading or the action bar.
const SECTION_GAP_PX: f32 = 18.;
/// The space above one activity event, and the indent of its body.
const EVENT_GAP_PX: f32 = 10.;
const EVENT_BODY_INDENT_PX: f32 = 24.;
/// The space between a heading's rule and its text.
const HEADING_RULE_GAP_PX: f32 = 12.;

/// What a panel button asks of the dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelAction {
    Compose(Mode),
    Close,
    Assign(&'static str),
    Diff,
    Open,
    ToggleChecks,
}

pub type OnAction = std::rc::Rc<dyn Fn(PanelAction, &mut Window, &mut App)>;

pub struct PanelInput<'a> {
    pub item: &'a GhItem,
    pub detail: Option<&'a GhItemDetail>,
    pub error: Option<&'a str>,
    pub account: &'a str,
    pub checks_open: bool,
    pub writing: bool,
    /// The composer, when it is open for this item.
    pub composer: Option<AnyElement>,
    pub now: i64,
    pub assignee_anchor: Entity<Anchor>,
    pub assignee_bottom_anchor: Entity<Anchor>,
}

fn footer_button(
    spec: ActionSpec,
    place: &'static str,
    input: &PanelInput<'_>,
    tokens: &Tokens,
    on: &OnAction,
) -> AnyElement {
    let callback = on.clone();
    let action = match spec.action {
        PanelAction::Assign(_) => PanelAction::Assign(place),
        action => action,
    };
    let button = action_button(
        (spec.id, place.len()),
        Intent::Neutral,
        Some(spec.icon),
        spec.label,
    )
    .accessibility_label(if spec.action == PanelAction::Close {
        "Close pull request…"
    } else {
        spec.label
    })
    .text_color(tokens.tone(spec.tone))
    .disabled(input.writing)
    .on_click(move |_, window, cx| callback(action, window, cx));
    match spec.action {
        PanelAction::Assign(_) => {
            let anchor = match place {
                "bottom" => &input.assignee_bottom_anchor,
                _ => &input.assignee_anchor,
            };
            anchored::trigger(anchor, button).into_any_element()
        }
        _ => button.into_any_element(),
    }
}

fn action_bar(input: &PanelInput<'_>, place: &'static str, tokens: &Tokens, on: &OnAction) -> Div {
    let item = input.item;
    let mut bar = h_flex().gap_2().flex_wrap();
    let add = |bar: Div,
               id: &'static str,
               label: &'static str,
               icon: IconName,
               intent: Intent,
               action: PanelAction| {
        let on = on.clone();
        bar.child(
            action_button((id, place.len()), intent, Some(icon), label)
                .disabled(input.writing)
                .on_click(move |_, window, cx| on(action, window, cx)),
        )
    };
    if can_review(item, input.detail) {
        bar = add(
            bar,
            "gh-approve",
            "Approve  A",
            IconName::Check,
            Intent::Accept,
            PanelAction::Compose(Mode::Approve),
        );
        bar = add(
            bar,
            "gh-changes",
            "Request changes  R",
            IconName::ShieldAlert,
            Intent::Caution,
            PanelAction::Compose(Mode::Changes),
        );
    }
    bar.children(
        footer_actions(
            item.kind,
            can_comment(input.detail),
            crate::close_pull_request::is_closable(item),
        )
        .into_iter()
        .map(|spec| footer_button(spec, place, input, tokens, on)),
    )
}

fn heading(text: impl Into<SharedString>, tokens: &Tokens) -> Div {
    div()
        .mt(px(SECTION_GAP_PX))
        .pt(px(HEADING_RULE_GAP_PX))
        .border_t_1()
        .border_color(tokens.line)
        .text_color(tokens.muted)
        .child(text.into())
}

fn glyph(glyph: Glyph, tokens: &Tokens) -> Div {
    div()
        .flex_shrink_0()
        .text_color(tokens.glyph(&glyph))
        .child(glyph.text)
}

fn note(text: impl Into<SharedString>, tokens: &Tokens) -> Div {
    div().text_color(tokens.muted).child(text.into())
}

/// Everything under the properties.
pub fn panel_extra(mut input: PanelInput<'_>, tokens: &Tokens, on: OnAction) -> AnyElement {
    let key = input.item.key.clone();
    let mut body = v_flex().gap_1().text_size(tokens.row_font);
    body = reviewers(body, &input, tokens);
    body = composer_or_actions(body, &mut input, tokens, &on);
    body = load_error(body, &input, tokens);
    body = checks(body, &input, tokens, &on);
    body = description(body, &input, tokens, &key);
    activity(body, &input, tokens, &key, &on).into_any_element()
}

/// The reviewers of a pull request.
fn reviewers(body: Div, input: &PanelInput<'_>, tokens: &Tokens) -> Div {
    if input.item.kind != ItemKind::Pr {
        return body;
    }
    let body = body.child(heading("Reviewers", tokens));
    match input.detail {
        None => body.child(note("…", tokens)),
        Some(detail) if detail.reviewers.is_empty() => body.child(note("None", tokens)),
        Some(detail) => body.children(detail.reviewers.iter().map(|reviewer| {
            let (badge, tag) = reviewer_badge(reviewer.state);
            h_flex()
                .gap_2()
                .child(glyph(badge, tokens))
                .child(
                    div()
                        .text_color(tokens.fg_strong)
                        .child(display_name(&reviewer.user).to_string()),
                )
                .child(note(tag, tokens))
        })),
    }
}

/// The action bar, or the composer in its place.
fn composer_or_actions(
    body: Div,
    input: &mut PanelInput<'_>,
    tokens: &Tokens,
    on: &OnAction,
) -> Div {
    let bar = match input.composer.take() {
        Some(composer) => div().mt(px(SECTION_GAP_PX)).child(composer),
        None => div()
            .mt(px(SECTION_GAP_PX))
            .child(action_bar(input, "top", tokens, on)),
    };
    body.child(bar)
}

/// Show a failed detail read alongside any content we can still display.
fn load_error(body: Div, input: &PanelInput<'_>, tokens: &Tokens) -> Div {
    match input.error {
        Some(error) => body.child(
            div()
                .id(format!("github-detail-error-{}", input.item.key))
                .test_support()
                .mt_2()
                .text_color(tokens.red)
                .child(rich::plain_text("error", error)),
        ),
        None => body,
    }
}

/// The checks of a pull request, collapsed to the failing ones above
/// `CHECKS_SHOWN`.
fn checks(body: Div, input: &PanelInput<'_>, tokens: &Tokens, on: &OnAction) -> Div {
    let Some(detail) = input.detail.filter(|d| !d.check_runs.is_empty()) else {
        return body;
    };
    let runs = &detail.check_runs;
    let collapsed = runs.len() > CHECKS_SHOWN && !input.checks_open;
    let body = body
        .child(heading(format!("Checks ({})", runs.len()), tokens))
        .when(runs.len() > CHECKS_SHOWN, |el| {
            el.child(checks_toggle(input, tokens, on))
        });
    body.children(
        runs.iter()
            .enumerate()
            .filter(|(_, check)| !collapsed || is_failed_check(check))
            .map(|(index, check)| check_row(index, check, tokens)),
    )
}

/// The summary line that shows all checks or only the failing ones.
fn checks_toggle(input: &PanelInput<'_>, _tokens: &Tokens, on: &OnAction) -> impl IntoElement {
    let on = on.clone();
    let summary = input
        .detail
        .map(|detail| check_summary(&detail.check_runs))
        .unwrap_or_default();
    let verb = if input.checks_open {
        "show failing"
    } else {
        "show all"
    };
    action_button(
        "gh-checks-toggle",
        Intent::Quiet,
        Some(IconName::ListChecks),
        format!("{summary} · {verb}"),
    )
    .selected(input.checks_open)
    .toggled(input.checks_open)
    .on_click(move |_, window, cx| on(PanelAction::ToggleChecks, window, cx))
}

fn check_row(
    index: usize,
    check: &subspace_framework::client_github::model::GhCheck,
    tokens: &Tokens,
) -> impl IntoElement {
    let (mark, word) = check_glyph(check.state);
    let url = check
        .url
        .clone()
        .filter(|url| links::safe_url(url).is_some());
    h_flex()
        .gap_2()
        .child(glyph(mark, tokens))
        .child(
            truncated::text(
                SharedString::from(format!("check-name:{}", check.name)),
                check.name.clone(),
            )
            .flex_1()
            .text_color(tokens.fg),
        )
        .child(note(word, tokens))
        .when_some(url, |row, url| {
            row.child(
                div()
                    .id(("gh-check-link", index))
                    .aria_label(crate::inspector::link_label(&check.name, &url))
                    .cursor_pointer()
                    .text_color(tokens.accent)
                    .child("↗")
                    .on_click(move |_, _, cx| {
                        links::open(&url, cx);
                    }),
            )
        })
}

/// The rendered description, or why there is none yet.
fn description(body: Div, input: &PanelInput<'_>, tokens: &Tokens, key: &str) -> Div {
    let body = body.child(heading("Description", tokens));
    match input.detail {
        None => body.child(feedback::loading_line(
            tokens,
            "github-detail-loading",
            "Loading…",
        )),
        Some(detail) if detail.body_html.trim().is_empty() => {
            body.child(note("No description.", tokens))
        }
        Some(detail) => body.child(div().child(rich_text(
            ElementId::Name(format!("gh-desc-{key}").into()),
            Format::Html,
            &detail.body_html,
        ))),
    }
}
