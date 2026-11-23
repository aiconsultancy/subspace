//! Reacting from a conversation: what each held message's reactions show
//! for this sync, sending and taking back our own, the keyboard's way into
//! the choices (`r` on a focused message), and the brief notices after a
//! refusal or a stop.
//!
//! Our reaction shows only once the client reports it: an `echo-message`
//! echo, or the client's local echo after a successful write. Nothing is
//! drawn ahead of that. A reaction goes to the conversation its message is
//! in; nothing here opens or sends a private message. Every reaction sent
//! goes through [`toggle`], which asks [`crate::reactions::gate`] first:
//! no reaction to a fleet agent in private, and a new stop reaction only
//! from its labelled menu item.

use std::rc::Rc;

use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, Window, div,
};
use subspace_client_irc::Command;
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, icon_button};
use subspace_framework::dashboard::anchored::Anchor;
use subspace_framework::dashboard::panel::IconName;

use super::Conversation;
use crate::reactions::choices::choices;
use crate::reactions::gate::{self, Gate, Via};
use crate::reactions::stop::{self, Stops};
use crate::reactions::{Shown, refusal_text};
use crate::transcript::{Row, reactions as pills, replies};

/// One conversation's reactions for the current sync and what it is
/// waiting to hear.
pub(crate) struct Reacting {
    /// Where the keyboard's reaction menu opens: the focused message.
    pub anchor: Entity<Anchor>,
    pub shown: Rc<Shown>,
    pub stops: Rc<Stops>,
    /// The network can react now.
    pub supported: bool,
    /// Stop reactions sent and not yet reported back, each resolved by
    /// its own message and exact reaction.
    pending: Vec<PendingStop>,
    /// The newest refusal already shown.
    seen_refusal: u64,
    /// The brief notice above the composer.
    pub notice: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingStop {
    msgid: String,
    reaction: String,
    ship: String,
}

impl Reacting {
    /// No reactions yet; refusals made before the conversation opened are
    /// not shown.
    pub fn new(seen_refusal: u64, cx: &mut App) -> Self {
        Self {
            anchor: cx.new(Anchor::new),
            shown: Rc::default(),
            stops: Rc::default(),
            supported: false,
            pending: Vec::new(),
            seen_refusal,
            notice: None,
        }
    }
}

/// What the rows showed before a refresh.
pub(super) struct Before {
    shown: Rc<Shown>,
    stops: Rc<Stops>,
    supported: bool,
}

/// Rebuilds what the buffer's messages show; returns what they showed.
pub(super) fn refresh(this: &mut Conversation, cx: &App) -> Before {
    let hub = this.hub.read(cx);
    let held = hub.state.buffer(&this.buffer);
    let shown = held
        .map(|held| hub.reactions.shown(&this.buffer, held))
        .unwrap_or_default();
    let stops = held
        .map(|held| {
            stop::stops(&stop::Sources {
                buffer: &this.buffer,
                held,
                statuses: &this.statuses.shown,
                roster: &this.roster,
            })
        })
        .unwrap_or_default();
    let supported = can_react(this, cx);
    let reacting = &mut this.reacting;
    let before = Before {
        shown: std::mem::replace(&mut reacting.shown, Rc::new(shown)),
        stops: std::mem::replace(&mut reacting.stops, Rc::new(stops)),
        supported: std::mem::replace(&mut reacting.supported, supported),
    };
    follow(this, cx);
    before
}

/// Rows whose reactions changed, to measure again one by one; every row
/// with reactions when support changed, as its controls come or go.
pub(super) fn changed_rows(
    this: &Conversation,
    rows: &[Row],
    before: &Before,
    cx: &App,
) -> Vec<usize> {
    let Some(held) = this.hub.read(cx).state.buffer(&this.buffer) else {
        return Vec::new();
    };
    let reacting = &this.reacting;
    let mut changed = replies::changed_rows(rows, &held.entries, &before.shown, &reacting.shown);
    changed.extend(replies::changed_rows(
        rows,
        &held.entries,
        &before.stops,
        &reacting.stops,
    ));
    if before.supported != reacting.supported {
        let everything = Shown::new();
        changed.extend(replies::changed_rows(
            rows,
            &held.entries,
            &everything,
            &reacting.shown,
        ));
    }
    changed
}

/// Whether reactions can be sent in this conversation now: never to the
/// server or to a fleet agent in private ([`gate::refusal`]).
fn can_react(this: &Conversation, cx: &App) -> bool {
    gate::refusal(&this.buffer.target, &this.roster).is_none()
        && this.hub.read(cx).reactions.supported(&this.buffer.network)
}

/// Shows a new refusal, and says each pending stop was sent once the
/// client reports our exact reaction on its message.
fn follow(this: &mut Conversation, cx: &App) {
    let hub = this.hub.read(cx);
    let refusal = hub
        .reactions
        .refusal(&this.buffer)
        .filter(|refusal| refusal.serial > this.reacting.seen_refusal)
        .cloned();
    let (reported, waiting): (Vec<PendingStop>, Vec<PendingStop>) =
        std::mem::take(&mut this.reacting.pending)
            .into_iter()
            .partition(|pending| reported(this, pending, cx));
    let reacting = &mut this.reacting;
    reacting.pending = waiting;
    if let Some(refusal) = refusal {
        reacting.seen_refusal = refusal.serial;
        reacting.notice = Some(refusal_text(&refusal.error));
        reacting
            .pending
            .retain(|pending| pending.msgid != refusal.msgid);
    }
    if let Some(pending) = reported.last() {
        reacting.notice = Some(stop_sent(&pending.reaction, &pending.ship));
    }
}

/// Whether the client now reports our `pending` reaction, exactly as sent,
/// on its message.
fn reported(this: &Conversation, pending: &PendingStop, cx: &App) -> bool {
    this.hub
        .read(cx)
        .reactions
        .reacted(&this.buffer, &pending.msgid, &pending.reaction)
}

/// What is known once a stop reaction was sent: only that it was.
pub(crate) fn stop_sent(reaction: &str, ship: &str) -> String {
    format!(
        "Sent {reaction} to ask {ship} to stop. The status line shows the result when Stargate reports it."
    )
}

/// Asks to add our reaction to the message `msgid` in this conversation,
/// or to take ours back. Every reaction Channels sends comes here, and
/// goes only if [`gate::gate`] lets it: `stop` names the agent when it was
/// chosen from the labelled stop item, so the notice can say it was sent
/// once it is.
pub(crate) fn toggle(
    this: &mut Conversation,
    (msgid, reaction): (String, String),
    add: bool,
    stop: Option<String>,
    cx: &mut Context<Conversation>,
) {
    let via = if stop.is_some() {
        Via::LabelledStop
    } else {
        Via::Ordinary
    };
    match gate::gate((&this.buffer.target, &this.roster), &reaction, add, via) {
        Gate::Send => send(this, (msgid, reaction), add, stop, cx),
        Gate::Explain => {
            let ship = ship_of(this, &msgid, cx);
            explain_stop(this, ship.as_deref(), cx);
        }
        Gate::Refuse(words) => {
            this.reacting.notice = Some(words);
            cx.notify();
        }
    }
}

/// Sends what [`toggle`] let through.
fn send(
    this: &mut Conversation,
    (msgid, reaction): (String, String),
    add: bool,
    stop: Option<String>,
    cx: &mut Context<Conversation>,
) {
    let buffer = this.buffer.clone();
    this.reacting.notice = None;
    if let Some(ship) = stop.filter(|_| add) {
        this.reacting.pending.push(PendingStop {
            msgid: msgid.clone(),
            reaction: reaction.clone(),
            ship,
        });
    }
    let command = if add {
        Command::React {
            buffer,
            msgid,
            reaction,
        }
    } else {
        Command::Unreact {
            buffer,
            msgid,
            reaction,
        }
    };
    this.send(command, cx);
    cx.notify();
}

/// The agent a stop on the message `msgid` would concern, when the app
/// recognises it.
fn ship_of(this: &Conversation, msgid: &str, cx: &App) -> Option<String> {
    let held = this.hub.read(cx).state.buffer(&this.buffer)?;
    let seq = held
        .entries
        .iter()
        .find(|entry| entry.line.id.as_deref() == Some(msgid))?
        .seq;
    this.reacting
        .stops
        .get(&seq)
        .map(|agent| agent.ship.clone())
}

/// Says where the deliberate stop is, instead of sending a stop reaction
/// chosen anywhere but its labelled item.
fn explain_stop(this: &mut Conversation, ship: Option<&str>, cx: &mut Context<Conversation>) {
    this.reacting.notice = Some(explanation(ship));
    cx.notify();
}

/// Why a stop reaction was not sent, and where the deliberate one is.
pub(crate) fn explanation(ship: Option<&str>) -> String {
    match ship {
        Some(ship) => format!(
            "To ask {ship} to stop, use Add reaction and choose under Stop {ship}, while its turn is running."
        ),
        None => "\u{270B} and \u{1F6D1} can stop an agent's turn, so Subspace sends them only from Add reaction, under Stop and the agent's name, on a running request to a fleet agent.".to_owned(),
    }
}

/// Opens the reaction choices for the keyboard-focused message, at that
/// message, when it can take reactions.
pub(super) fn open_choices(
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
    this.reacting.anchor.update(cx, |anchor, cx| {
        anchor.show_menu_with(window, cx, move |menu, _, _| {
            pills::menu_items(menu, &offered, &msgid, &me)
        });
    });
}

/// The brief notice above the composer, with Dismiss.
pub(super) fn notice(this: &Conversation, cx: &mut Context<Conversation>) -> Option<AnyElement> {
    let text = this.reacting.notice.clone()?;
    let tokens = Tokens::current(cx);
    Some(
        h_flex()
            .id("irc-reaction-notice")
            .role(Role::Status)
            .aria_label(SharedString::from(text.clone()))
            .w_full()
            .gap_2()
            .px_2()
            .py_1()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .text_color(tokens.fg)
                    .child(text),
            )
            .child(
                icon_button(
                    "irc-reaction-notice-dismiss",
                    Intent::Quiet,
                    IconName::X,
                    "Dismiss",
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.reacting.notice = None;
                    cx.notify();
                })),
            )
            .test_support()
            .into_any_element(),
    )
}
