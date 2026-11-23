//! The transcript's keyboard focus ([`crate::transcript::focus`]): the
//! actions, their keys and what each does to the conversation. A focused
//! issue reference shows its tooltip card under its message.

use std::ops::Range;

use gpui_kit::{
    AnyView, App, Context, Div, FocusHandle, InteractiveElement as _, KeyBinding, Stateful, Window,
    actions, div,
};
use subspace_framework::dashboard::{KeyHelp, links};

use super::{Conversation, ConversationEvent};
use crate::issues;
use crate::markup::Ref;
use crate::pull_requests;
use crate::text::Opens;
use crate::transcript;
use crate::transcript::actions::{self, MessageAction};
use crate::transcript::focus::{self, TranscriptFocus};

actions!(
    irc_transcript,
    [
        FocusTranscript,
        PreviousMessage,
        NextMessage,
        NextReference,
        PreviousReference,
        OpenReference,
        OpenAttachment,
        LeaveTranscript,
        ReactToMessage,
        ReplyToMessage,
        MakeIssueFromMessage,
        RecordDecisionFromMessage,
    ]
);

/// The conversation's key context, around the transcript and the input.
pub(crate) const CONVERSATION: &str = "IrcConversation";
/// The transcript's key context while it has focus.
pub(crate) const TRANSCRIPT: &str = "IrcTranscript";

/// The help overlay's rows: the list's keys, then the transcript's.
pub(crate) const KEYS: &[KeyHelp] = &[
    KeyHelp::new("j / ↓", "Next row"),
    KeyHelp::new("k / ↑", "Previous row"),
    KeyHelp::new("Enter", "Open the buffer, or fold or unfold a network"),
    KeyHelp::new(
        "Shift+Tab in the message input",
        "Move to the newest message",
    ),
    KeyHelp::new("↑ / ↓ on a message", "Previous or next message"),
    KeyHelp::new(
        "Tab / Shift+Tab on a message",
        "Next or previous link or reference",
    ),
    KeyHelp::new("Enter on a link or reference", "Open it"),
    KeyHelp::new(
        "Enter on a message with replies or a request",
        "Open its thread in the inspector",
    ),
    KeyHelp::new("Enter / Space on an attachment", "Open it"),
    KeyHelp::new("r on a message", "Add or take back a reaction"),
    KeyHelp::new("t on a message", "Reply in its thread"),
    KeyHelp::new("i on a message", "Make an issue from it"),
    KeyHelp::new("d on a message", "Record a decision from it"),
    KeyHelp::new("Esc on a message", "Back to the message input"),
];

/// The transcript's focus handle, the focused message and reference while
/// it has focus, and the focused issue reference's card.
pub(crate) struct Keyboard {
    pub handle: FocusHandle,
    pub at: Option<TranscriptFocus>,
    pub card: Option<AnyView>,
    /// The message under the pointer, whose toolbar shows as the focused
    /// one's does.
    pub hovered: Option<i64>,
}

impl Keyboard {
    pub fn new(cx: &mut App) -> Self {
        Self {
            handle: cx.focus_handle(),
            at: None,
            card: None,
            hovered: None,
        }
    }
}

/// The transcript's focusable frame and its actions.
pub(super) fn transcript(this: &Conversation, cx: &mut Context<Conversation>) -> Stateful<Div> {
    div()
        .id("irc-transcript-focus")
        .track_focus(&this.keyboard.handle)
        .key_context(TRANSCRIPT)
        .on_action(cx.listener(|this, _: &PreviousMessage, window, cx| step(this, -1, window, cx)))
        .on_action(cx.listener(|this, _: &NextMessage, window, cx| step(this, 1, window, cx)))
        .on_action(cx.listener(|this, _: &NextReference, window, cx| {
            step_reference(this, true, window, cx);
        }))
        .on_action(cx.listener(|this, _: &PreviousReference, window, cx| {
            step_reference(this, false, window, cx);
        }))
        .on_action(cx.listener(|this, _: &OpenReference, window, cx| open(this, window, cx)))
        .on_action(cx.listener(|this, _: &OpenAttachment, window, cx| {
            if let Some(key) = focused_upload(this, cx) {
                crate::uploads::viewer::open(this, key, window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &LeaveTranscript, window, cx| leave(this, window, cx)))
        .on_action(cx.listener(|this, _: &ReactToMessage, window, cx| {
            super::reacting::open_choices(this, window, cx);
        }))
        .on_action(cx.listener(|this, _: &ReplyToMessage, window, cx| {
            act(this, MessageAction::Reply, window, cx);
        }))
        .on_action(cx.listener(|this, _: &MakeIssueFromMessage, window, cx| {
            act(this, MessageAction::MakeIssue, window, cx);
        }))
        .on_action(
            cx.listener(|this, _: &RecordDecisionFromMessage, window, cx| {
                act(this, MessageAction::RecordDecision, window, cx);
            }),
        )
}

/// Notes the message under the pointer, which shows its toolbar as the
/// focused one's does.
pub(crate) fn hover(
    this: &mut Conversation,
    seq: i64,
    hovered: bool,
    cx: &mut Context<Conversation>,
) {
    let next = match (hovered, this.keyboard.hovered) {
        (true, _) => Some(seq),
        (false, Some(held)) if held == seq => None,
        (false, held) => held,
    };
    if next != this.keyboard.hovered {
        this.keyboard.hovered = next;
        cx.notify();
    }
}

/// Does a toolbar action for the focused message, when its toolbar offers
/// it.
fn act(
    this: &mut Conversation,
    action: MessageAction,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    let Some(seq) = this.keyboard.at.map(|at| at.seq) else {
        return;
    };
    let offered = {
        let context = this.row_context(cx);
        let mentions = super::mentions(this, cx);
        let held = this.hub.read(cx).state.buffer(&this.buffer);
        held.and_then(|held| held.entries.iter().find(|entry| entry.seq == seq))
            .map(|entry| {
                let names = !transcript::named_issues(entry, &mentions).is_empty();
                actions::offered(entry, context.actions, names)
            })
            .unwrap_or_default()
    };
    if offered.contains(&action) {
        actions::run(this, action, seq, window, cx);
    }
}

/// The focused message and reference, for tests.
#[cfg(test)]
pub(crate) fn focused(this: &Conversation) -> Option<TranscriptFocus> {
    this.keyboard.at
}

/// Whether a focused issue reference shows its card, for tests.
#[cfg(test)]
pub(crate) fn has_card(this: &Conversation) -> bool {
    this.keyboard.card.is_some()
}

/// The line numbers of the message rows, oldest first.
fn messages(this: &Conversation, cx: &App) -> Vec<i64> {
    let (rows, _) = this.current_rows(cx);
    let state = &this.hub.read(cx).state;
    state
        .buffer(&this.buffer)
        .map(|held| focus::messages(&rows, &held.entries))
        .unwrap_or_default()
}

/// Message `seq`'s links and references in reading order.
fn targets(this: &Conversation, seq: i64, cx: &App) -> Vec<(Range<usize>, Opens)> {
    let mentions = super::mentions(this, cx);
    let hub = this.hub.read(cx);
    hub.state
        .buffer(&this.buffer)
        .and_then(|held| held.entries.iter().find(|entry| entry.seq == seq))
        .map(|entry| transcript::message_targets(entry, &mentions, Some(hub)))
        .unwrap_or_default()
}

/// The focused link or reference, if one is.
fn focused_target(this: &Conversation, cx: &App) -> Option<Opens> {
    let at = this.keyboard.at?;
    let mut targets = targets(this, at.seq, cx);
    let index = at.reference.filter(|index| *index < targets.len())?;
    Some(targets.swap_remove(index).1)
}

/// Scrolls message `seq` into view.
fn scroll_to_message(this: &Conversation, seq: i64, cx: &mut App) {
    let (rows, _) = this.current_rows(cx);
    let held = this.hub.read(cx).state.buffer(&this.buffer);
    let at = held.and_then(|held| held.entries.iter().position(|entry| entry.seq == seq));
    if let Some(index) = at.and_then(|at| transcript::row_of(&rows, at)) {
        this.scroll
            .state
            .update(cx, |state, cx| _ = state.scroll_to_item(index, cx));
    }
}

/// Shift+Tab in the message input moves into the transcript, ahead of the
/// text area's own outdent; the rest act while the transcript has focus.
pub(crate) fn bind_keys(cx: &mut App) {
    crate::uploads::viewer::bind(cx);
    let input = Some("IrcConversation > Input");
    let transcript = Some(TRANSCRIPT);
    cx.bind_keys([
        KeyBinding::new("shift-tab", FocusTranscript, input),
        KeyBinding::new("up", PreviousMessage, transcript),
        KeyBinding::new("down", NextMessage, transcript),
        KeyBinding::new("tab", NextReference, transcript),
        KeyBinding::new("shift-tab", PreviousReference, transcript),
        KeyBinding::new("enter", OpenReference, transcript),
        KeyBinding::new("space", OpenAttachment, transcript),
        KeyBinding::new("escape", LeaveTranscript, transcript),
        KeyBinding::new("r", ReactToMessage, transcript),
        KeyBinding::new("t", ReplyToMessage, transcript),
        KeyBinding::new("i", MakeIssueFromMessage, transcript),
        KeyBinding::new("d", RecordDecisionFromMessage, transcript),
        KeyBinding::new(
            "escape",
            subspace_framework::dashboard::find_in_page::Close,
            Some("IrcTranscript && !SubspaceModal && !SubspaceRawKeys"),
        ),
    ]);
}

/// Focuses the newest message.
pub(super) fn enter(this: &mut Conversation, window: &mut Window, cx: &mut Context<Conversation>) {
    let cursor = focus::newest(&messages(this, cx));
    move_to(this, cursor, window, cx);
    window.focus(&this.keyboard.handle, cx);
}

/// Focuses the message `delta` messages away.
pub(super) fn step(
    this: &mut Conversation,
    delta: isize,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    let messages = messages(this, cx);
    let cursor = this
        .keyboard
        .at
        .and_then(|cursor| focus::step_message(&messages, cursor, delta));
    move_to(this, cursor, window, cx);
}

/// Focuses the focused message's next or previous link or reference.
pub(super) fn step_reference(
    this: &mut Conversation,
    forward: bool,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    let cursor = this.keyboard.at.map(|cursor| {
        let count = targets(this, cursor.seq, cx).len();
        focus::step_reference(cursor, count, forward)
    });
    move_to(this, cursor, window, cx);
}

/// Opens the focused link in the browser or the focused reference as a
/// click on it does.
pub(super) fn open(this: &mut Conversation, window: &mut Window, cx: &mut Context<Conversation>) {
    match focused_target(this, cx) {
        Some(Opens::Url(url)) => _ = links::open(&url, cx),
        Some(Opens::Ref(found)) => cx.emit(ConversationEvent::Open(found)),
        Some(Opens::Upload(key)) => crate::uploads::viewer::open(this, key, window, cx),
        None => open_thread(this, cx),
    }
}

/// Opens the focused message's thread in the inspector, as choosing its
/// reply count or status line does: where the inspector has a Thread tab
/// and the message has replies or a request's status line.
fn open_thread(this: &Conversation, cx: &mut Context<Conversation>) {
    let Some(seq) = this.keyboard.at.map(|at| at.seq) else {
        return;
    };
    let threaded = this.replies.contains_key(&seq) || this.statuses.shown.contains_key(&seq);
    if !threaded || !crate::thread::has_tab(&this.roster, &this.buffer) {
        return;
    }
    let id = this
        .hub
        .read(cx)
        .state
        .buffer(&this.buffer)
        .and_then(|held| held.entries.iter().find(|entry| entry.seq == seq))
        .and_then(|entry| entry.line.id.clone());
    if let Some(id) = id {
        cx.emit(ConversationEvent::OpenThread(id));
    }
}

/// The upload thumbnail the keyboard is on in message `seq`, if any.
pub(crate) fn focused_upload(this: &Conversation, cx: &App) -> Option<String> {
    match focused_target(this, cx)? {
        Opens::Upload(key) => Some(key),
        _ => None,
    }
}

/// Back to the message input, leaving nothing focused in the transcript.
pub(super) fn leave(this: &mut Conversation, window: &mut Window, cx: &mut Context<Conversation>) {
    move_to(this, None, window, cx);
    this.focus_composer(window, cx);
}

/// Focus left the transcript, as by a click elsewhere. The attachment
/// viewer keeps the cursor: closing it returns to the same thumbnail.
pub(super) fn blurred(this: &mut Conversation, cx: &mut Context<Conversation>) {
    if this.upload_viewer.is_some() {
        return;
    }
    this.keyboard.at = None;
    this.keyboard.card = None;
    cx.notify();
}

fn move_to(
    this: &mut Conversation,
    cursor: Option<TranscriptFocus>,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    this.keyboard.at = cursor;
    let card = match focused_target(this, cx) {
        Some(Opens::Ref(Ref::Issue(identifier))) => Some(issues::tooltip(
            &identifier,
            this.hub.downgrade(),
            window,
            cx,
        )),
        Some(Opens::Ref(Ref::PullRequest(found))) => Some(pull_requests::tooltip(
            &found,
            this.hub.downgrade(),
            window,
            cx,
        )),
        _ => None,
    };
    this.keyboard.card = card;
    if let Some(cursor) = cursor {
        scroll_to_message(this, cursor.seq, cx);
    }
    cx.notify();
}
