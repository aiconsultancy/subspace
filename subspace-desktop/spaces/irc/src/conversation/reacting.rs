//! Reacting from a conversation: what each held message's reactions show
//! for this sync, sending and taking back our own, the keyboard's way into
//! the choices (`r` on a focused message), and the brief notices after a
//! refusal or a stop.
//!
//! Our reaction shows only once the client reports it: an `echo-message`
//! echo, or the client's local echo after a successful write. Nothing is
//! drawn ahead of that. A reaction goes to the conversation its message is
//! in; nothing here opens or sends a private message.

use std::rc::Rc;

use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, Window, div,
};
use subspace_client_irc::reactions::agent_stop::reaction_key;
use subspace_client_irc::{Command, Target};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, icon_button};
use subspace_framework::dashboard::anchored::Anchor;
use subspace_framework::dashboard::panel::IconName;

use super::Conversation;
use crate::reactions::choices::choices;
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
    /// A stop reaction sent and not yet reported back.
    pending: Option<PendingStop>,
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
            pending: None,
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
        .map(|held| stop::stops(&this.buffer, held, &this.statuses.shown))
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

/// Whether reactions can be sent in this conversation now.
fn can_react(this: &Conversation, cx: &App) -> bool {
    !matches!(this.buffer.target, Target::Server)
        && this.hub.read(cx).reactions.supported(&this.buffer.network)
}

/// Shows a new refusal, and says a pending stop was sent once the client
/// reports our reaction on that message.
fn follow(this: &mut Conversation, cx: &App) {
    let hub = this.hub.read(cx);
    let refusal = hub
        .reactions
        .refusal(&this.buffer)
        .filter(|refusal| refusal.serial > this.reacting.seen_refusal)
        .cloned();
    let reported = this
        .reacting
        .pending
        .as_ref()
        .is_some_and(|pending| reported(this, pending, cx));
    let reacting = &mut this.reacting;
    if let Some(refusal) = refusal {
        reacting.seen_refusal = refusal.serial;
        reacting.notice = Some(refusal_text(&refusal.error));
        if reacting
            .pending
            .as_ref()
            .is_some_and(|pending| pending.msgid == refusal.msgid)
        {
            reacting.pending = None;
        }
    }
    if reported && let Some(pending) = reacting.pending.take() {
        reacting.notice = Some(stop_sent(&pending.reaction, &pending.ship));
    }
}

/// Whether the client now reports our `pending` reaction on its message.
fn reported(this: &Conversation, pending: &PendingStop, cx: &App) -> bool {
    let key = reaction_key(&pending.reaction);
    let held = this.hub.read(cx).state.buffer(&this.buffer);
    let seq = held.and_then(|held| {
        held.entries
            .iter()
            .find(|entry| entry.line.id.as_deref() == Some(pending.msgid.as_str()))
            .map(|entry| entry.seq)
    });
    seq.and_then(|seq| this.reacting.shown.get(&seq))
        .is_some_and(|summaries| {
            summaries
                .iter()
                .any(|summary| summary.mine() && reaction_key(&summary.reaction) == key)
        })
}

/// What is known once a stop reaction was sent: only that it was.
pub(crate) fn stop_sent(reaction: &str, ship: &str) -> String {
    format!(
        "Sent {reaction} to ask {ship} to stop. The status line shows the result when Stargate reports it."
    )
}

/// Sends our reaction to the message `msgid` in this conversation, or
/// takes ours back. `stop` names the agent when it was chosen from the
/// stop group, so the notice can say it was sent once it is.
pub(crate) fn toggle(
    this: &mut Conversation,
    (msgid, reaction): (String, String),
    add: bool,
    stop: Option<String>,
    cx: &mut Context<Conversation>,
) {
    let buffer = this.buffer.clone();
    this.reacting.notice = None;
    this.reacting.pending = stop.filter(|_| add).map(|ship| PendingStop {
        msgid: msgid.clone(),
        reaction: reaction.clone(),
        ship,
    });
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

/// A stop reaction's pill on an agent message is not a stop button: say
/// where the deliberate one is.
pub(crate) fn explain_stop(
    this: &mut Conversation,
    ship: Option<&str>,
    cx: &mut Context<Conversation>,
) {
    let ship = ship.unwrap_or("the agent");
    this.reacting.notice = Some(format!(
        "To ask {ship} to stop, use Add reaction and choose under Stop {ship}, while its turn is running."
    ));
    cx.notify();
}

/// Opens the reaction choices for the keyboard-focused message, at that
/// message, when it can take reactions.
pub(super) fn open_choices(
    this: &mut Conversation,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    let Some(seq) = this.keyboard.at.map(|at| at.seq) else {
        return;
    };
    let msgid = this
        .hub
        .read(cx)
        .state
        .buffer(&this.buffer)
        .and_then(|held| held.entries.iter().find(|entry| entry.seq == seq))
        .and_then(|entry| entry.line.id.clone());
    let Some(msgid) = msgid.filter(|_| this.reacting.supported) else {
        this.reacting.notice = Some("This message cannot take reactions here.".to_owned());
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
