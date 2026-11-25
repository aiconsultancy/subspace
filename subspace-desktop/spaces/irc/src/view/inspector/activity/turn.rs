//! One turn as the view draws it: a heading that opens and closes it,
//! then its items in the order they began: model steps with their settled
//! tokens and cost, tool calls with their input, progress, output and
//! errors, and reply and reasoning text, each whole. An item of a kind
//! or shape this version does not know shows as its JSON.

use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, Context, IntoElement as _, ParentElement as _, SharedString, Styled as _,
};
use subspace_client_fleet::activity::{
    ActivityEntry, ActivityTokens, ActivityTurn, TurnStatus, TurnSummary,
};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;

use super::Selection;
use super::parts::caption;
use crate::Irc;

/// The turns, newest first.
pub(super) fn list(selection: &Selection, cx: &Context<Irc>) -> Vec<AnyElement> {
    let Some(activity) = selection.join.activity() else {
        return Vec::new();
    };
    activity
        .turns
        .iter()
        .rev()
        .map(|turn| view(selection, turn, cx))
        .collect()
}

/// Whether a turn shows its items: a running turn does, until closed by
/// hand; any other once opened.
pub(super) fn is_open(selection: &Selection, turn: &TurnSummary) -> bool {
    (turn.status == TurnStatus::Running) != selection.toggled.contains(&turn.id)
}

pub(super) fn view(selection: &Selection, turn: &ActivityTurn, cx: &Context<Irc>) -> AnyElement {
    let id = turn.summary.id.clone();
    let open = is_open(selection, &turn.summary);
    let icon = if open {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };
    let toggle_id = id.clone();
    let header = action_button(
        SharedString::from(format!("irc-activity-turn:{id}")),
        Intent::Quiet,
        Some(icon),
        heading(&turn.summary),
    )
    .on_click(cx.listener(move |board, _, _, cx| super::toggle(board, &toggle_id, cx)));
    let reconnected = (turn.summary.reconnects > 0).then(|| {
        caption(
            &format!("irc-activity-reconnected:{id}"),
            "Some changes may be missing (the stream reconnected)",
            cx,
        )
    });
    let items = open.then(|| {
        turn.items
            .iter()
            .map(|entry| self::entry(selection, &id, entry, cx))
    });
    v_flex()
        .w_full()
        .min_w_0()
        .items_start()
        .gap_2()
        .child(header)
        .children(reconnected)
        .children(items.into_iter().flatten())
        .into_any_element()
}

/// `Running turn · 3 steps · 1,234 tokens · $0.0420`.
pub(super) fn heading(turn: &TurnSummary) -> String {
    let status = match turn.status {
        TurnStatus::Running => "Running turn",
        TurnStatus::Completed => "Completed turn",
        TurnStatus::Failed => "Failed turn",
        TurnStatus::Cancelled => "Cancelled turn",
        TurnStatus::Unknown => "Turn",
    };
    match &turn.usage {
        Some(usage) => format!(
            "{status} · {} steps · {} tokens · ${:.4}",
            usage.steps,
            total(&usage.tokens),
            usage.cost
        ),
        None => status.to_owned(),
    }
}

pub(super) fn total(tokens: &ActivityTokens) -> u64 {
    tokens.input + tokens.output + tokens.reasoning + tokens.cache_read + tokens.cache_write
}

pub(super) fn entry(
    selection: &Selection,
    turn: &str,
    entry: &ActivityEntry,
    cx: &Context<Irc>,
) -> AnyElement {
    let id = format!("irc-activity:{turn}:{}", entry.base().id);
    match entry {
        ActivityEntry::Item(item) => super::item::item(&id, item, &selection.parts, cx),
        ActivityEntry::Paged(paged) => super::paged::paged(selection, turn, paged, cx),
        ActivityEntry::Unknown(unknown) => super::item::unknown(&id, unknown, &selection.parts, cx),
    }
}
