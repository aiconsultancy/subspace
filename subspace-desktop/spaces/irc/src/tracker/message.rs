//! A message's tracker actions: **Make issue** creates an issue from it and
//! **Record decision** records its words as a decision on an issue. Each
//! opens a small dialog first, where the person can edit the title and
//! choose the project or the issue; only its button (or Return in its
//! title) writes. The message's text is quoted as text: a fenced block in
//! an issue's description, so nothing in it becomes a link or a mention.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{
    App, AppContext as _, Context, Entity, FocusHandle, KeyBinding, Subscription, Window, actions,
};
use subspace_client_irc::Target;

use super::flow::{self, Binding, Origin};
pub(crate) use super::quote::Kind;
use super::quote::{Quoted, choices, decision_draft, issue_draft};
use super::write::{Job, title_of};
use crate::conversation::Conversation;

actions!(irc_tracker, [CancelTrackerDialog]);

mod view;
pub(crate) use view::render;

const CONTEXT: &str = "IrcTrackerDialog";

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new(
        "escape",
        CancelTrackerDialog,
        Some(CONTEXT),
    )]);
}

/// An open action dialog.
pub(crate) struct Dialog {
    kind: Kind,
    quoted: Quoted,
    pub(crate) title: Entity<InputState>,
    choices: Vec<String>,
    pub(crate) chosen: Option<String>,
    pub(crate) error: Option<String>,
    /// The account, organisation and tracker the dialog opened under: its
    /// button writes only while they are still current.
    binding: Binding,
    previous_focus: Option<FocusHandle>,
    _subscription: Subscription,
}

/// Opens `kind`'s dialog for message `seq`, when the action works here.
pub(crate) fn open(
    conversation: &mut Conversation,
    kind: Kind,
    seq: i64,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    if conversation.tracker.busy || conversation.tracker.dialog.is_some() {
        return;
    }
    let Some(facts) = super::facts(conversation, None, cx) else {
        return;
    };
    let Some((quoted, named)) = message(conversation, seq, cx) else {
        return;
    };
    let Some(binding) = flow::bind(conversation, cx) else {
        return;
    };
    let (choices, chosen) = choices(kind, &named, &facts.in_play, &facts.projects);
    if choices.is_empty() {
        conversation.reacting.notice = Some(match kind {
            Kind::RecordDecision => {
                "Name an issue in the conversation first, or use /decision <issue> <decision>."
                    .to_owned()
            }
            Kind::MakeIssue => super::NO_TRACKER.to_owned(),
        });
        cx.notify();
        return;
    }
    let title = cx.new(|cx| {
        let mut input = InputState::new(window, cx);
        input.set_value(title_of(&quoted.text), window, cx);
        input
    });
    let subscription =
        cx.subscribe_in(&title, window, |this, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                confirm(this, window, cx);
            }
        });
    let previous_focus = window.focused(cx);
    title.update(cx, |input, cx| input.focus(window, cx));
    conversation.tracker.dialog = Some(Dialog {
        kind,
        quoted,
        title,
        choices,
        chosen,
        error: None,
        binding,
        previous_focus,
        _subscription: subscription,
    });
    cx.notify();
}

/// Message `seq` quoted, and the issues its text names.
fn message(conversation: &Conversation, seq: i64, cx: &App) -> Option<(Quoted, Vec<String>)> {
    let hub = conversation.hub.read(cx);
    let entry = hub
        .state
        .buffer(&conversation.buffer)?
        .entries
        .iter()
        .find(|entry| entry.seq == seq)?;
    let mentions = crate::conversation::mentions(conversation, cx);
    let named = crate::transcript::named_issues(entry, &mentions);
    let network = hub.state.network(&conversation.buffer.network).map_or_else(
        || conversation.buffer.network.0.clone(),
        |known| known.name.clone(),
    );
    let place = match &conversation.buffer.target {
        Target::Channel(name) | Target::Query(name) => format!("{name} on {network}"),
        Target::Server => network,
    };
    let quoted = Quoted::of(entry, place);
    (!quoted.text.trim().is_empty()).then_some((quoted, named))
}

/// Closes the dialog and gives the keyboard back.
pub(crate) fn close(
    conversation: &mut Conversation,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    if let Some(dialog) = conversation.tracker.dialog.take() {
        if let Some(focus) = dialog.previous_focus {
            focus.focus(window, cx);
        }
        cx.notify();
    }
}

/// After the write: a landed one closes the dialog, a refused one keeps it
/// with its text so the person can try again.
pub(crate) fn finished(
    conversation: &mut Conversation,
    landed: bool,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    if landed {
        close(conversation, window, cx);
    } else if let Some(dialog) = conversation.tracker.dialog.as_mut() {
        dialog.error = conversation.reacting.notice.clone();
    }
}

fn choose(conversation: &mut Conversation, choice: String, cx: &mut Context<Conversation>) {
    if let Some(dialog) = conversation.tracker.dialog.as_mut() {
        dialog.chosen = Some(choice);
        dialog.error = None;
        cx.notify();
    }
}

/// The dialog's button: checks the title and the choice, then writes.
pub(crate) fn confirm(
    conversation: &mut Conversation,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    if conversation.tracker.busy {
        return;
    }
    let context = flow::context(conversation, cx);
    let Some(dialog) = conversation.tracker.dialog.as_mut() else {
        return;
    };
    let title = dialog.title.read(cx).value().to_string();
    let job = match (&dialog.chosen, dialog.kind) {
        (None, Kind::RecordDecision) => Err("Choose the issue the decision is on.".to_owned()),
        (None, Kind::MakeIssue) => Err("Choose the project for the issue.".to_owned()),
        (Some(project), Kind::MakeIssue) => {
            issue_draft(&dialog.quoted, project, &title).map(Job::Create)
        }
        (Some(issue), Kind::RecordDecision) => {
            decision_draft(&dialog.quoted, issue, &title, context).map(Job::Decide)
        }
    };
    let job = job.and_then(|job| {
        flow::still_bound(&dialog.binding, conversation.hub.read(cx)).map(|client| (job, client))
    });
    match job {
        Ok((job, client)) => {
            dialog.error = None;
            flow::start_with(
                conversation,
                (job, Origin::Dialog),
                Some(client),
                window,
                cx,
            );
        }
        Err(reason) => {
            dialog.error = Some(reason);
            cx.notify();
        }
    }
}
