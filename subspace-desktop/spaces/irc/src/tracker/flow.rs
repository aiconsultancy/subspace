//! Running a tracker write from the conversation: a typed command on
//! Return, or a message action from its dialog. One write at a time; the
//! composer or the dialog waits while it runs and keeps its text when it
//! is refused.

use std::sync::Arc;

use gpui_kit::{AppContext as _, Context, Window};
use subspace_api_client::Client;
use subspace_client_irc::Target;

use super::failed::Failed;
use super::message::Dialog;
use super::write::{self, Done, Job};
use super::{available, facts, plan, typed};
use crate::conversation::Conversation;

/// Why a tracker command with attachments was kept.
pub(crate) const ATTACHMENTS: &str =
    "Tracker commands don't take attachments. Send the files on their own.";
/// Why a draft whose last write was never confirmed is not sent again.
pub(crate) const UNCONFIRMED_AGAIN: &str = "The tracker didn't confirm the last try, so this may already be saved. Check the tracker; to send it again anyway, change the command first.";
/// Why a dialog's button wrote nothing after the account, organisation
/// or tracker changed.
pub(crate) const SCOPE_CHANGED: &str = "You switched account, organisation or tracker since this opened, so nothing was written. Close it and open it again.";

/// A conversation's tracker writes: whether one is running, and the open
/// message action's dialog.
#[derive(Default)]
pub(crate) struct State {
    pub busy: bool,
    pub dialog: Option<Dialog>,
    /// A typed command whose write the tracker never confirmed: it may
    /// have landed, so the same draft is not sent again.
    pub unconfirmed: Option<String>,
    /// The issues asked for again after a write, which tests read back.
    #[cfg(test)]
    pub refreshed: Vec<String>,
}

/// What a dialog opened under: the API client of the signed-in account and
/// organisation then, and the tracker (its profile and API origin).
pub(crate) struct Binding {
    client: Arc<Client>,
    tracker: (String, String),
}

/// What `conversation` would write under now, when it can write at all.
pub(crate) fn bind(conversation: &Conversation, cx: &mut Context<Conversation>) -> Option<Binding> {
    let tracker = tracker_of(conversation.hub.read(cx))?;
    let client = conversation.hub.update(cx, |hub, _| hub.tracker.client())?;
    Some(Binding { client, tracker })
}

/// The client `binding` holds, while the account, organisation and tracker
/// it opened under are still current; otherwise why not.
pub(crate) fn still_bound(binding: &Binding, hub: &crate::hub::Hub) -> Result<Arc<Client>, String> {
    let same_tracker = tracker_of(hub).is_some_and(|now| now == binding.tracker);
    if same_tracker && !binding.client.account_changed() {
        Ok(Arc::clone(&binding.client))
    } else {
        Err(SCOPE_CHANGED.to_owned())
    }
}

/// The tracker the hub's Issues scope names: its profile and API origin.
fn tracker_of(hub: &crate::hub::Hub) -> Option<(String, String)> {
    let scope = hub.issues.scope()?;
    Some((scope.profile_id.clone(), scope.api_origin.clone()))
}

/// Where a write came from, which keeps its text if it is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    Composer,
    Dialog,
}

/// Runs `text` when it is a tracker command and returns true; a command
/// that cannot run keeps the draft and says why. False for anything else.
pub(crate) fn submit(
    conversation: &mut Conversation,
    text: &str,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) -> bool {
    let Some(typed) = typed(text) else {
        return false;
    };
    if conversation.tracker.unconfirmed.as_deref() == Some(text) {
        conversation.draft.entity.update(cx, |input, cx| {
            input.set_error(UNCONFIRMED_AGAIN.to_owned(), cx)
        });
        return true;
    }
    let planned = if conversation.tray.is_empty() {
        let refusal = available(&conversation.buffer, &conversation.hub.read(cx).issues);
        match refusal {
            Ok(()) => facts(conversation, typed.issue.clone(), cx)
                .ok_or(super::NO_TRACKER.to_owned())
                .and_then(|facts| plan::plan(&typed, &facts)),
            Err(reason) => Err(reason.to_owned()),
        }
    } else {
        Err(ATTACHMENTS.to_owned())
    };
    match planned {
        Ok(plan) => {
            let job = Job::from_plan(plan, context(conversation, cx));
            start(conversation, job, Origin::Composer, window, cx);
        }
        Err(reason) => conversation
            .draft
            .entity
            .update(cx, |input, cx| input.set_error(reason, cx)),
    }
    true
}

/// Where a decision recorded here comes from, for its context: the
/// channel's and network's names, which other people chose, as a code
/// span.
pub(crate) fn context(conversation: &Conversation, cx: &gpui_kit::App) -> String {
    let state = &conversation.hub.read(cx).state;
    let network = state.network(&conversation.buffer.network).map_or_else(
        || conversation.buffer.network.0.clone(),
        |known| known.name.clone(),
    );
    let place = match &conversation.buffer.target {
        Target::Channel(name) | Target::Query(name) => format!("{name} on {network}"),
        Target::Server => network,
    };
    format!(
        "Recorded in Channels from {}.",
        super::quote::code_span(&place)
    )
}

/// Starts `job` in the background with the current account's client; the
/// notice says it is running, then what came of it.
pub(crate) fn start(
    conversation: &mut Conversation,
    job: Job,
    origin: Origin,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    let client = conversation.hub.update(cx, |hub, _| hub.tracker.client());
    start_with(conversation, (job, origin), client, window, cx);
}

/// Starts `job` from `origin` in the background with `client`.
pub(crate) fn start_with(
    conversation: &mut Conversation,
    (job, origin): (Job, Origin),
    client: Option<Arc<Client>>,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    if conversation.tracker.busy {
        return;
    }
    let scope = conversation.hub.read(cx).issues.scope();
    let client = match super::destination::matching(client, scope) {
        Ok(client) => client,
        Err(reason) => {
            finish(conversation, Err(reason.into()), origin, window, cx);
            return;
        }
    };
    conversation.tracker.busy = true;
    conversation.reacting.notice = Some(job.running());
    if origin == Origin::Composer {
        conversation
            .draft
            .entity
            .update(cx, |input, cx| input.set_busy(true, cx));
    }
    cx.spawn_in(window, async move |this, cx| {
        let result = cx
            .background_spawn(async move { write::run(&client, &job) })
            .await;
        let _ = this.update_in(cx, |this, window, cx| {
            finish(this, result, origin, window, cx);
        });
    })
    .detach();
    cx.notify();
}

/// Says what came of a write; a landed one empties the composer or closes
/// the dialog and asks for the issue again, a refused one keeps the text.
fn finish(
    conversation: &mut Conversation,
    result: Result<Done, Failed>,
    origin: Origin,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    conversation.tracker.busy = false;
    let outcome = Outcome::of(&result);
    let notice = match result {
        Ok(done) => {
            refresh(conversation, &done.changed, cx);
            done.notice
        }
        Err(failed) => failed.words,
    };
    conversation.reacting.notice = Some(notice);
    match origin {
        Origin::Composer => composer_finished(conversation, outcome, window, cx),
        Origin::Dialog => super::message::finished(conversation, outcome, window, cx),
    }
    cx.notify();
}

/// What came of a write, for where it came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Landed,
    Refused,
    /// Sent, but never confirmed: it may have landed.
    Unconfirmed,
}

impl Outcome {
    fn of(result: &Result<Done, Failed>) -> Self {
        match result {
            Ok(_) => Self::Landed,
            Err(failed) if failed.unconfirmed => Self::Unconfirmed,
            Err(_) => Self::Refused,
        }
    }
}

/// The composer after its command's write: emptied once it landed; kept
/// otherwise, and not sent again as it stands once it went unconfirmed.
fn composer_finished(
    conversation: &mut Conversation,
    outcome: Outcome,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    let input = conversation.draft.entity.clone();
    let text = input.read(cx).text(cx);
    conversation.tracker.unconfirmed = (outcome == Outcome::Unconfirmed).then_some(text);
    input.update(cx, |input, cx| {
        input.set_busy(false, cx);
        if outcome == Outcome::Landed {
            input.clear(window, cx);
        }
    });
}

/// Asks the tracker for `identifier` again, so tiles and **In play here**
/// show what the write changed.
fn refresh(conversation: &mut Conversation, identifier: &str, cx: &mut Context<Conversation>) {
    let identifier = identifier.to_owned();
    #[cfg(test)]
    conversation.tracker.refreshed.push(identifier.clone());
    conversation
        .hub
        .update(cx, |hub, cx| crate::issues::refresh(hub, identifier, cx));
}
