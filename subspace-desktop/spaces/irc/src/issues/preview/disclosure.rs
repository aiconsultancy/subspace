use crate::{conversation::summaries, state::Entry, transcript::RowContext};
use gpui_kit::component::Selectable as _;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::contracts::issues::IssueSummary;
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, icon_button},
    animated_collapsible::AnimatedCollapsible,
    fields,
};
use subspace_framework::lucide::IconName;

pub(super) fn toggle(
    entry: &Entry,
    issue: &str,
    context: &RowContext,
    cx: &App,
) -> impl IntoElement {
    let seq = entry.seq;
    let key = summaries::identity(entry);
    let view = context.conversation.clone();
    let expanded = view
        .upgrade()
        .and_then(|view| {
            view.read(cx)
                .summaries
                .panel(&key)
                .map(|panel| panel.issue == issue && panel.motion.is_open())
        })
        .unwrap_or(false);
    let verb = if expanded { "Hide" } else { "Show" };
    let label = format!("{verb} summary of {issue}");
    let glyph = if expanded {
        IconName::ChevronUp
    } else {
        IconName::ChevronDown
    };
    let issue = issue.to_owned();
    icon_button(
        SharedString::from(format!("irc-line:{seq}:issue-summary:{issue}:toggle")),
        Intent::Quiet,
        glyph,
        label,
    )
    .compact()
    .focus_on_click()
    .open(expanded)
    .aria_expanded(expanded)
    .on_click(move |_, _, cx| {
        cx.stop_propagation();
        summaries::toggle(&view, key.clone(), &issue, cx);
    })
}

pub(super) fn panel(
    entry: &Entry,
    cards: &[&IssueSummary],
    context: &RowContext,
    cx: &App,
) -> Option<AnyElement> {
    let seq = entry.seq;
    let key = summaries::identity(entry);
    let view = context.conversation.upgrade()?;
    let panel = view.read(cx).summaries.panel(&key)?;
    if !panel.motion.is_visible(cx) {
        return None;
    }
    let summary = cards
        .iter()
        .find(|summary| summary.identifier == panel.issue)?;
    let text = summary.summary.as_deref().unwrap_or("No summary yet");
    let id = SharedString::from(format!("irc-line:{seq}:issue-summary:{}", panel.issue));
    let content = div()
        .pt_1()
        .child(fields::note(&Tokens::current(cx), text.to_owned()));
    Some(
        div()
            .id(SharedString::from(format!("irc-line:{seq}:summary-panel")))
            .test_support()
            .aria_label(text.to_owned())
            .w_full()
            .min_w_0()
            .child(
                AnimatedCollapsible::new(id, &panel.motion, content)
                    .on_resize(summaries::resize(context.conversation.clone(), key)),
            )
            .into_any_element(),
    )
}
