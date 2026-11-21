use crate::{CopilotAgents, logs};
use gpui_kit::{Context, Window};
use subspace_framework::dashboard::full_text;

pub(super) fn open(
    board: &mut CopilotAgents,
    hit: &full_text::Hit,
    permit: full_text::ReadPermit,
    _window: &mut Window,
    cx: &mut Context<CopilotAgents>,
) {
    if !permit.valid() || hit.target.scope.board != "github-copilot-agents" {
        return;
    }
    let Some(service) = subspace_framework::dashboard::full_text_service(cx) else {
        return;
    };
    if hit.target.scope.app != service.app_id() {
        return;
    }
    let Some((task_key, session_id)) =
        serde_json::from_str::<(String, String)>(&hit.document.navigation).ok()
    else {
        return;
    };
    if hit.target.id != hit.document.id || session_id != hit.document.id {
        return;
    }
    let Some(log) = hit
        .document
        .payload
        .as_deref()
        .and_then(|payload| serde_json::from_str(payload).ok())
    else {
        return;
    };
    if board.row(&task_key).is_none() {
        return;
    }
    // The hit opens its task as a tab, as any opened row does, then shows
    // the indexed log there.
    board.open_item(&task_key, cx);
    board.log = Some(logs::OpenLog {
        task_key,
        session_id: session_id.clone(),
        cache_key: logs::cache_key(&session_id, "retained"),
        state: logs::LogState::Ready(log),
    });
    cx.notify();
}
