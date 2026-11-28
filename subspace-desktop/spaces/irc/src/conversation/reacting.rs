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
use std::time::{Duration, Instant};

use gpui_kit::{App, AppContext as _, Context, Entity, Window};
use subspace_client_irc::Command;
use subspace_framework::dashboard::anchored::Anchor;

use super::Conversation;

mod menu;
mod notice;
use crate::reactions::gate::{self, Gate, Via};
use crate::reactions::stop::{self, Stops, explanation, stop_sent};
use crate::reactions::{Shown, refusal_text};
use crate::transcript::{Row, replies};
pub(super) use menu::open_choices;
pub(crate) use menu::open_choices_for;
pub(super) use notice::notice;

/// One conversation's reactions for the current sync and what it is
/// waiting to hear.
pub(crate) struct Reacting {
    pub anchors: Anchors,
    pub shown: Rc<Shown>,
    pub stops: Rc<Stops>,
    /// The network can react now.
    pub supported: bool,
    /// What `shown` was read from: the book's revision for this
    /// conversation and the held lines then.
    shown_from: Option<ShownFrom>,
    /// Stop reactions sent and not yet reported back, each resolved by
    /// its own message and exact reaction.
    pending: Vec<PendingStop>,
    /// The newest refusal already shown.
    seen_refusal: u64,
    /// The brief notice above the composer.
    pub notice: Option<String>,
}

/// The book's revision for a conversation and its held lines' extent
/// (how many, the first and the last by sequence): lines are only added
/// and dropped, never changed in place, so an equal extent holds the same
/// lines.
type ShownFrom = (u64, (usize, Option<i64>, Option<i64>));

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingStop {
    msgid: String,
    reaction: String,
    ship: String,
    /// The hub's latest live addition of ours when it was sent: only an
    /// echo after it acknowledges this stop.
    after: u64,
    /// When it was sent; one whose echo never comes is let go after
    /// [`STOP_ECHO_WAIT`].
    sent: Instant,
}

/// How long a sent stop waits for its echo before it is let go, its
/// notice unsaid.
const STOP_ECHO_WAIT: Duration = Duration::from_secs(120);

impl Reacting {
    /// No reactions yet; refusals made before the conversation opened are
    /// not shown.
    pub fn new(seen_refusal: u64, cx: &mut App) -> Self {
        Self {
            anchors: Anchors {
                keyboard: cx.new(Anchor::new),
                reaction: cx.new(Anchor::new),
                stop: cx.new(Anchor::new),
            },
            shown: Rc::default(),
            stops: Rc::default(),
            supported: false,
            shown_from: None,
            pending: Vec::new(),
            seen_refusal,
            notice: None,
        }
    }
}

/// Menus outlive the virtual row or the toolbar that opened them.
#[derive(Clone)]
pub(crate) struct Anchors {
    pub keyboard: Entity<Anchor>,
    pub reaction: Entity<Anchor>,
    pub stop: Entity<Anchor>,
}

impl Anchors {
    pub(crate) fn dismiss(&self, window: &mut Window, cx: &mut App) {
        for anchor in [&self.keyboard, &self.reaction, &self.stop] {
            anchor.update(cx, |anchor, cx| anchor.dismiss(window, cx));
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
    let from = held.map(|held| (hub.reactions.revision(&this.buffer), extent(held)));
    let shown = match held {
        Some(_) if from == this.reacting.shown_from => Rc::clone(&this.reacting.shown),
        Some(held) => Rc::new(hub.reactions.shown(&this.buffer, held)),
        None => Rc::default(),
    };
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
    reacting.shown_from = from;
    let before = Before {
        shown: std::mem::replace(&mut reacting.shown, shown),
        stops: std::mem::replace(&mut reacting.stops, Rc::new(stops)),
        supported: std::mem::replace(&mut reacting.supported, supported),
    };
    follow(this, cx);
    before
}

/// The extent of `held`'s lines, for [`ShownFrom`].
fn extent(held: &crate::state::Buffer) -> (usize, Option<i64>, Option<i64>) {
    let seq = |entry: &crate::state::Entry| entry.seq;
    (
        held.entries.len(),
        held.entries.front().map(seq),
        held.entries.back().map(seq),
    )
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
    reacting.pending = still_waiting(waiting, Instant::now());
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

/// Whether the client has reported our `pending` reaction, exactly as
/// sent, on its message since it was sent: its live echo, read before the
/// book's display limits, so a message already showing every reaction it
/// can still acknowledges a stop.
fn reported(this: &Conversation, pending: &PendingStop, cx: &App) -> bool {
    let target = (pending.msgid.as_str(), pending.reaction.as_str());
    this.hub
        .read(cx)
        .reactions
        .echoed_since(pending.after, &this.buffer, target)
}

/// The stops still waiting for their echo at `now`, none for longer than
/// [`STOP_ECHO_WAIT`].
fn still_waiting(pending: Vec<PendingStop>, now: Instant) -> Vec<PendingStop> {
    pending
        .into_iter()
        .filter(|stop| now.saturating_duration_since(stop.sent) < STOP_ECHO_WAIT)
        .collect()
}

/// Asks to add our reaction to the message `msgid` in this conversation,
/// or to take ours back. Every reaction Channels sends comes here, and
/// goes only if [`gate::gate`] lets it. `stop` is set when it was chosen
/// from the labelled stop item; that item counts only while the message
/// still concerns a running roster agent now ([`menu::labelled_stop`]), so a
/// menu left open while the turn ended or the roster changed sends
/// nothing, and the notice names the agent read now.
pub(crate) fn toggle(
    this: &mut Conversation,
    (msgid, reaction): (String, String),
    add: bool,
    stop: Option<String>,
    cx: &mut Context<Conversation>,
) {
    let stop = menu::labelled_stop(this, &msgid, stop, cx);
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
        let after = this.hub.read(cx).reactions.echo_serial();
        this.reacting.pending.push(PendingStop {
            msgid: msgid.clone(),
            reaction: reaction.clone(),
            ship,
            after,
            sent: Instant::now(),
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

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{PendingStop, STOP_ECHO_WAIT, still_waiting};

    #[test]
    fn a_stop_whose_echo_never_comes_is_let_go() {
        let sent = Instant::now();
        let pending = vec![PendingStop {
            msgid: "r1".into(),
            reaction: "\u{270B}".into(),
            ship: "astra".into(),
            after: 0,
            sent,
        }];
        let soon = sent + Duration::from_secs(5);
        assert_eq!(still_waiting(pending.clone(), soon), pending);
        assert!(still_waiting(pending, sent + STOP_ECHO_WAIT).is_empty());
    }
}
