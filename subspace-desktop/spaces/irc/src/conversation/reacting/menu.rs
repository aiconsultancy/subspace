//! The reaction menu at a message: `r` on the focused message, **Add
//! reaction** and the toolbar's React open the same choices, with the stop
//! group when its conditions hold. Every choice goes through
//! [`super::toggle`], which checks a stop item's choice again when it is
//! made ([`labelled_stop`]).

use gpui_kit::{App, Context, Window};

use crate::conversation::Conversation;
use crate::reactions::choices::choices;
use crate::reactions::{gate, stop};
use crate::transcript::reactions as pills;

/// Opens the reaction choices for the keyboard-focused message, at that
/// message, when it can take reactions.
pub(in crate::conversation) fn open_choices(
    this: &mut Conversation,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    if let Some(seq) = this.keyboard.at.map(|at| at.seq) {
        open_choices_for(this, seq, window, cx);
    }
}

/// Opens the reaction choices for message `seq` at that message, as `r`,
/// **Add reaction** and the toolbar's React do, when it can take
/// reactions: the same menu, with the stop group when its conditions hold.
pub(crate) fn open_choices_for(
    this: &mut Conversation,
    seq: i64,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    let msgid = this
        .hub
        .read(cx)
        .state
        .buffer(&this.buffer)
        .and_then(|held| held.entries.iter().find(|entry| entry.seq == seq))
        .and_then(|entry| entry.line.id.clone());
    let Some(msgid) = msgid.filter(|_| this.reacting.supported) else {
        let refused = gate::refusal(&this.buffer.target, &this.roster);
        this.reacting.notice =
            Some(refused.unwrap_or_else(|| "This message cannot take reactions here.".to_owned()));
        cx.notify();
        return;
    };
    let summaries = this.reacting.shown.get(&seq).cloned().unwrap_or_default();
    let ship = this
        .reacting
        .stops
        .get(&seq)
        .filter(|agent| agent.running)
        .map(|agent| agent.ship.clone());
    let offered = choices(&summaries, ship.as_deref());
    let me = this.me.clone();
    let keyboard = this.keyboard.at.is_some_and(|at| at.seq == seq);
    let anchor = if keyboard {
        &this.reacting.anchor
    } else {
        &this.reacting.hover_anchor
    };
    anchor.clone().update(cx, |anchor, cx| {
        anchor.show_menu_with(window, cx, move |menu, _, _| {
            pills::menu_items(menu, &offered, &msgid, &me)
        });
    });
}

/// The agent a choice from the labelled stop item stops, checked again
/// when it is chosen: the live roster, the held message and its status
/// line must still make it a running agent's request or reply. The
/// menu's own `chosen` ship only says the item was the labelled one; the
/// name comes from this check.
pub(super) fn labelled_stop(
    this: &Conversation,
    msgid: &str,
    chosen: Option<String>,
    cx: &App,
) -> Option<String> {
    chosen?;
    let hub = this.hub.read(cx);
    let held = hub.state.buffer(&this.buffer)?;
    let roster = hub.roster(&this.buffer.network, cx);
    let sources = stop::Sources {
        buffer: &this.buffer,
        held,
        statuses: &this.statuses.shown,
        roster: &roster,
    };
    stop::concerning(&sources, msgid)
        .filter(|agent| agent.running)
        .map(|agent| agent.ship)
}
