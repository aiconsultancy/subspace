//! A queued send completes once the runtime wrote it and every reference
//! has echoed back as our own line; then the tray clears, and the composer
//! too if it still holds the sent text. A refused send, or one the
//! connection lost while writing, keeps the tray and the draft and says
//! why. A queued send is never retried automatically.
use super::{Phase, changed, error};
use crate::conversation::Conversation;
use gpui_kit::Context;
use subspace_client_irc::SendResult;

const ECHO_LOOKBACK: usize = 50;

pub(crate) fn follow(this: &mut Conversation, cx: &mut Context<Conversation>) {
    let Phase::Queued { id, .. } = &this.tray.phase else {
        return;
    };
    let id = *id;
    let answer = this.hub.update(cx, |hub, _| hub.state.take_send(id));
    match answer.as_ref().map(SendResult::problem) {
        Some(Some(problem)) => return refuse(this, problem, cx),
        Some(None) => accept(this),
        None => {}
    }
    let Phase::Queued {
        keys,
        accepted: true,
        ..
    } = &this.tray.phase
    else {
        return;
    };
    let seen = this
        .hub
        .read(cx)
        .state
        .buffer(&this.buffer)
        .is_some_and(|b| {
            let own: Vec<&str> = b
                .entries
                .iter()
                .rev()
                .filter(|e| e.line.own)
                .take(ECHO_LOOKBACK)
                .map(|e| e.line.text.as_str())
                .collect();
            keys.iter()
                .all(|key| own.iter().any(|text| text.contains(key.as_str())))
        });
    if !seen {
        return;
    }
    let Phase::Queued { text, window, .. } = std::mem::take(&mut this.tray.phase) else {
        return;
    };
    this.tray.items.clear();
    this.tray.notice = None;
    clear_sent(this, text, window, cx);
    changed(this, cx);
}

/// The runtime wrote every line: the text joins the recall history.
fn accept(this: &mut Conversation) {
    if let Phase::Queued { text, accepted, .. } = &mut this.tray.phase {
        *accepted = true;
        if !text.trim().is_empty() {
            this.draft.recall.push(text.clone());
        }
    }
}

/// The runtime sent nothing, or lost the connection while writing: the
/// tray and the draft stay as they were, `problem` says why, and only the
/// next explicit Enter sends again.
fn refuse(this: &mut Conversation, problem: &str, cx: &mut Context<Conversation>) {
    this.tray.phase = Phase::Idle;
    error(this, problem, cx);
    changed(this, cx);
}

/// Clears the composer only while it still holds exactly the sent text, so
/// anything typed during the upload stays.
fn clear_sent(
    this: &Conversation,
    text: String,
    window: gpui_kit::AnyWindowHandle,
    cx: &mut Context<Conversation>,
) {
    if text.is_empty() {
        return;
    }
    let input = this.draft.entity.clone();
    cx.defer(move |cx| {
        let _ = window.update(cx, |_, window, cx| {
            input.update(cx, |input, cx| {
                if input.text(cx).trim() == text {
                    input.clear(window, cx);
                }
            });
        });
    });
}

/// × or Dismiss on a queued tray whose echo never came: clears the tray
/// without sending anything again.
pub(crate) fn dismiss(this: &mut Conversation, cx: &mut Context<Conversation>) {
    if !matches!(this.tray.phase, Phase::Queued { .. }) {
        return;
    }
    this.tray.phase = Phase::Idle;
    this.tray.items.clear();
    this.tray.notice = None;
    changed(this, cx);
}
