//! Pure selection and deduplication of chat events that need user attention.

use subspace_agent::{ChatEvent, ChatId, FailureCategory, TurnFailure};

use super::Pending;
use super::tab_run::ShownFailure;
use super::tabs::Tab;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatAttentionKind {
    TurnCompleted,
    WaitingForInput,
}

pub enum ChatPanelEvent {
    Close,
    Attention {
        kind: ChatAttentionKind,
        chat: ChatId,
    },
}

pub(super) struct TabUpdate {
    pub(super) transcript: Option<ChatEvent>,
    pub(super) attention: Option<ChatAttentionKind>,
}

/// What one chat's event changes in its own tab.
pub(super) fn apply_chat_event(tab: &mut Tab, event: ChatEvent) -> TabUpdate {
    match event {
        request @ (ChatEvent::Approval { .. } | ChatEvent::Question { .. }) => {
            apply_request_event(tab, request)
        }
        selected @ (ChatEvent::Usage(_)
        | ChatEvent::Mode(_)
        | ChatEvent::SelectedModel { .. }
        | ChatEvent::ModelConfirmed { .. }
        | ChatEvent::SelectedAgent(_)
        | ChatEvent::SelectedMode(_)) => apply_selection(tab, selected),
        canvas @ (ChatEvent::CanvasOpened(_)
        | ChatEvent::CanvasRemembered(_)
        | ChatEvent::CanvasClosed(_)) => apply_canvas(tab, canvas),
        event => apply_tab_event(tab, event),
    }
}

fn apply_tab_event(tab: &mut Tab, event: ChatEvent) -> TabUpdate {
    let mut update = empty_update();
    match event {
        ChatEvent::Ready { session_id } => {
            tab.ready();
            tab.session = Some(session_id);
        }
        ChatEvent::History(entries) => tab.entries = entries,
        entry @ ChatEvent::Entry(_) => {
            recovered(tab);
            update.transcript = Some(entry);
        }
        delta @ ChatEvent::Delta { .. } => {
            recovered(tab);
            tab.resume();
            update.transcript = Some(delta);
        }
        ChatEvent::TurnDone => {
            update.attention = tab.end().then_some(ChatAttentionKind::TurnCompleted);
        }
        ChatEvent::AttachmentsAccepted(ids) => {
            tab.images.retain(|image| !ids.contains(&image.id));
        }
        // A notice: a turn the runtime is still running goes on.
        ChatEvent::Error(error) => tab.set_error(error),
        ChatEvent::Failure(failure) => apply_failure(tab, *failure),
        ChatEvent::Title(title) => tab.title = title.into(),
        _ => {}
    }
    update
}

/// A failure is shown until the next turn. The runtime ends a turn that
/// failed with its own `TurnDone`, and a failed steering message leaves the
/// turn running. A stopped or silent runtime disconnects the chat at once:
/// it cannot take this chat's messages until a new one has it ready.
fn apply_failure(tab: &mut Tab, failure: TurnFailure) {
    let details = super::failure_view::tab_error_details(&failure, tab);
    tab.errors.accept_failure(details);
    let reconnect = failure.category.needs_reconnect();
    tab.failure = Some(ShownFailure::new(failure));
    if reconnect {
        tab.disconnect();
    }
}

/// A stopped runtime finishes no turn: every tab disconnects, so the status
/// line ends, open requests go and Reconnect is enabled. No tab is ready
/// until a new runtime opens it. Returns whether a turn ended.
pub(super) fn end_turns(tabs: &mut [Tab], stopped: bool) -> bool {
    let mut ended = false;
    if stopped {
        for tab in tabs {
            ended |= tab.disconnect();
        }
    }
    ended
}

/// Output after a "not responding" failure means the runtime recovered and
/// re-armed the turn: the failure no longer holds, the chat is usable and
/// the turn runs again until its own end.
fn recovered(tab: &mut Tab) {
    let silent = tab
        .failure
        .as_ref()
        .is_some_and(|shown| shown.failure.category == FailureCategory::NotResponding);
    if silent {
        tab.failure = None;
        tab.ready();
        tab.resume();
    }
}

fn apply_canvas(tab: &mut Tab, event: ChatEvent) -> TabUpdate {
    match event {
        ChatEvent::CanvasOpened(canvas) => {
            remember_canvas(tab, canvas.clone());
            if let Err(error) = tab.canvases.open(canvas) {
                tab.set_error(error);
            }
        }
        ChatEvent::CanvasRemembered(canvas) => remember_canvas(tab, canvas),
        ChatEvent::CanvasClosed(id) => tab.canvases.close(&id),
        _ => {}
    }
    empty_update()
}

fn remember_canvas(tab: &mut Tab, canvas: std::sync::Arc<subspace_agent::canvas::Canvas>) {
    if tab.canvas_history.len() >= subspace_agent::canvas::MAX_CANVAS_HISTORY {
        let evicted = tab
            .canvas_history
            .keys()
            .find(|id| !tab.canvases.items.iter().any(|open| &open.id == *id))
            .cloned();
        if let Some(id) = evicted {
            tab.canvas_history.remove(&id);
        }
    }
    tab.canvas_history.insert(canvas.id.clone(), canvas);
}

fn apply_request_event(tab: &mut Tab, event: ChatEvent) -> TabUpdate {
    match event {
        ChatEvent::Approval {
            request_id,
            summary,
        } => add_request(
            tab,
            request_id.clone(),
            Pending::Approval {
                id: request_id,
                summary,
            },
        ),
        ChatEvent::Question {
            request_id,
            question,
            choices,
        } => add_request(
            tab,
            request_id.clone(),
            Pending::Question {
                id: request_id,
                text: question,
                choices,
            },
        ),
        _ => empty_update(),
    }
}

fn add_request(tab: &mut Tab, request_id: String, pending: Pending) -> TabUpdate {
    if !tab.notified_requests.insert(request_id) {
        return empty_update();
    }
    tab.pending.push(pending);
    TabUpdate {
        transcript: None,
        attention: tab
            .running()
            .is_some_and(|turn| turn.notify)
            .then_some(ChatAttentionKind::WaitingForInput),
    }
}

/// What the runtime confirmed for one chat's own settings.
fn apply_selection(tab: &mut Tab, event: ChatEvent) -> TabUpdate {
    match event {
        ChatEvent::Usage(usage) => tab.usage = usage,
        ChatEvent::Mode(yolo) => {
            tab.yolo = yolo;
            tab.confirm_yolo = false;
        }
        ChatEvent::SelectedModel { model, effort } => {
            tab.model = Some(model);
            tab.effort = effort;
        }
        ChatEvent::ModelConfirmed { model } => tab.model = Some(model),
        ChatEvent::SelectedAgent(agent) => tab.agent = Some(agent),
        ChatEvent::SelectedMode(mode) => tab.mode = mode,
        _ => {}
    }
    empty_update()
}

fn empty_update() -> TabUpdate {
    TabUpdate {
        transcript: None,
        attention: None,
    }
}

#[cfg(test)]
mod attachment_tests {
    use super::apply_chat_event;
    use crate::agent_chat::tabs::Tab;
    use subspace_agent::{ChatEvent, ImageAttachment};

    fn image(id: u64) -> ImageAttachment {
        ImageAttachment {
            id,
            data: vec![id as u8].into(),
            mime_type: "image/png".into(),
            display_name: format!("{id}.png"),
        }
    }

    #[test]
    fn failed_send_keeps_drafts_and_acceptance_removes_only_sent_ids() {
        let mut tab = Tab::new(1);
        tab.images = vec![image(10), image(20)];
        apply_chat_event(&mut tab, ChatEvent::Error("send failed".into()));
        assert_eq!(
            tab.images.iter().map(|image| image.id).collect::<Vec<_>>(),
            vec![10, 20],
            "runtime rejection must retain every draft"
        );
        apply_chat_event(&mut tab, ChatEvent::AttachmentsAccepted(vec![10]));
        assert_eq!(
            tab.images.iter().map(|image| image.id).collect::<Vec<_>>(),
            vec![20],
            "an accepted send must not remove a newer or unrelated draft"
        );
    }
}
