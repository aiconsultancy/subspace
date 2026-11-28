//! One turn as the view draws it: a heading that opens and closes it,
//! then its items in the order they began: model steps with their settled
//! tokens and cost, tool calls with their input, progress, output and
//! errors, and reply and reasoning text, each whole, within the room the
//! budget leaves ([`super::draw`]). An item of a kind or shape this
//! version does not know shows as its JSON.

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
use super::draw::Plan;
use super::parts::{Draw, caption};
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
        .map(|turn| view(selection, &selection.plan, turn, cx))
        .collect()
}

/// Whether a turn shows its items: a running turn does, until closed by
/// hand; any other once opened.
pub(super) fn is_open(selection: &Selection, turn: &TurnSummary) -> bool {
    (turn.status == TurnStatus::Running) != selection.toggled.contains(&turn.id)
}

pub(super) fn view(
    selection: &Selection,
    plan: &Plan,
    turn: &ActivityTurn,
    cx: &Context<Irc>,
) -> AnyElement {
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
            .map(|entry| self::entry(selection, plan, &id, entry, cx))
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

/// One item: its body, or, when that would pass the room left, its
/// heading and **Show**, or the body last asked for in parts
/// ([`super::draw`]).
pub(super) fn entry(
    selection: &Selection,
    plan: &Plan,
    turn: &str,
    entry: &ActivityEntry,
    cx: &Context<Irc>,
) -> AnyElement {
    let id = format!("irc-activity:{turn}:{}", entry.base().id);
    let key = (turn.to_owned(), entry.base().id.clone());
    let held_back = plan.held_back.get(&key).copied();
    let pieced = plan.pieced.as_ref().filter(|pieced| pieced.key == key);
    let prepared = super::draw::prepared(selection, &key);
    let draw = Draw {
        shown: &selection.parts,
        prepared: &prepared,
    };
    match (entry, held_back, pieced) {
        (ActivityEntry::Paged(paged), ..) => {
            super::paged::paged(selection, (turn, paged, pieced, held_back), &draw, cx)
        }
        (entry, Some(bytes), _) => super::draw::held_back(&id, (&key, entry, bytes), cx),
        (entry, None, Some(pieced)) => super::draw::pieced(&id, (entry, pieced), cx),
        (entry, None, None) => super::item::whole(&id, entry, &draw, cx),
    }
}
