//! Retained project move preview. Numbers are committed only after the exact list is confirmed.
mod picker;
pub(crate) mod view;
use crate::Issues;
use gpui_kit::{AppContext as _, Context, FocusHandle, Window};
use subspace_api_client::{
    hub::IssueWriteScope, issue_moves::MovePreview, issue_writes::IssueSaveError,
};

pub(crate) struct ProjectMove {
    pub(crate) id: String,
    focus: FocusHandle,
    pub(crate) scope: IssueWriteScope,
    pub(crate) confirming: bool,
    destination: Option<String>,
    phase: Phase,
    serial: u64,
    previous_focus: Option<FocusHandle>,
}
enum Phase {
    Choosing,
    Loading,
    Preview(MovePreview),
    Sending,
    Unconfirmed(MovePreview),
    Failed(IssueSaveError),
}
impl ProjectMove {
    fn busy(&self) -> bool {
        matches!(self.phase, Phase::Loading | Phase::Sending)
    }
}

// qual:allow(coupling, oi) reason: "The move workflow owns async result fences and coordinates the board, scope and focus."
impl Issues {
    pub(crate) fn open_project_move(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_edit_issue() || self.editor.is_some() || self.project_move.is_some() {
            return;
        }
        let Some(detail) = self.issue_view.snapshot.as_ref().and_then(|s| s.detail()) else {
            return;
        };
        let Some(scope) = self.hub.as_ref().and_then(|h| h.issue_write_scope()) else {
            return;
        };
        let previous_focus = window.focused(cx);
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        self.project_move = Some(ProjectMove {
            focus,
            confirming: false,
            id: detail.fields.id.clone(),
            scope,
            destination: None,
            phase: Phase::Choosing,
            serial: 0,
            previous_focus,
        });
        cx.notify();
    }
    fn close_project_move(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .project_move
            .as_ref()
            .is_some_and(|m| matches!(m.phase, Phase::Sending))
        {
            return;
        }
        if let Some(state) = self.project_move.take() {
            state
                .previous_focus
                .unwrap_or_else(|| self.focus.clone())
                .focus(window, cx);
        }
        self.menu = None;
        cx.notify();
    }
    fn load_move_preview(&mut self, cx: &mut Context<Self>) {
        let (Some(state), Some(hub)) = (self.project_move.as_mut(), self.hub.clone()) else {
            return;
        };
        let Some(destination) = state.destination.clone() else {
            return;
        };
        if state.busy() || matches!(state.phase, Phase::Unconfirmed(_)) {
            return;
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        state.serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (serial, id, scope) = (state.serial, state.id.clone(), state.scope.clone());
        state.phase = Phase::Loading;
        cx.spawn(async move |this, cx| {
            let check_scope = scope.clone();
            let result = cx
                .background_spawn(async move { hub.preview_issue_move(&scope, &id, &destination) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(state) = this.current_move(serial, &check_scope) {
                    state.phase = match result {
                        Ok(preview) => Phase::Preview(preview),
                        Err(error) => Phase::Failed(error),
                    };
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
    fn current_move(&mut self, serial: u64, scope: &IssueWriteScope) -> Option<&mut ProjectMove> {
        if self.hub.as_ref()?.issue_write_scope().as_ref() != Some(scope) {
            return None;
        }
        self.project_move
            .as_mut()
            .filter(|s| s.serial == serial && &s.scope == scope)
    }
    fn confirm_project_move(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.send_project_move(false, window, cx);
    }
    fn check_project_move(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.send_project_move(true, window, cx);
    }
    fn send_project_move(&mut self, checking: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(state), Some(hub)) = (self.project_move.as_mut(), self.hub.clone()) else {
            return;
        };
        let preview = match (&state.phase, checking) {
            (Phase::Preview(preview), false) | (Phase::Unconfirmed(preview), true) => {
                preview.clone()
            }
            _ => return,
        };
        let (scope, serial, id) = (state.scope.clone(), state.serial, state.id.clone());
        state.phase = Phase::Sending;
        cx.spawn_in(window, async move |this, cx| {
            let sent = preview.clone();
            let check_scope = scope.clone();
            let result = cx
                .background_spawn(async move {
                    if checking
                        && subspace_api_client::backend() == subspace_api_client::Backend::Nett
                    {
                        // Every previewed issue is read again; nothing is resent.
                        hub.verify_issue_move(&scope, &sent)
                            .map(|result| result.issue)
                    } else if checking {
                        hub.latest_issue(&scope, &id)
                    } else {
                        hub.move_issue(&scope, &sent).map(|result| result.issue)
                    }
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                let Some(state) = this.current_move(serial, &check_scope) else {
                    return;
                };
                match result {
                    Ok(issue)
                        if preview.root_moved_to(&issue.fields.identifier)
                            && issue.fields.project_id == preview.destination.id =>
                    {
                        this.finish_project_move(issue.fields.identifier, window, cx)
                    }
                    Ok(_) => state.phase = Phase::Failed(IssueSaveError::Conflict),
                    Err(IssueSaveError::Conflict) => {
                        state.phase = Phase::Failed(IssueSaveError::Conflict);
                    }
                    Err(_) if checking => state.phase = Phase::Unconfirmed(preview),
                    Err(IssueSaveError::Unconfirmed) => state.phase = Phase::Unconfirmed(preview),
                    Err(error) => state.phase = Phase::Failed(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn finish_project_move(
        &mut self,
        identifier: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(state) = &mut self.project_move {
            state.phase = Phase::Choosing;
        }
        self.close_project_move(window, cx);
        self.reload_list(cx);
        self.open_detail(identifier, cx);
    }
}

// qual:allow(coupling, oi) reason: "The explicit review action coordinates retained state and keyboard focus."
impl Issues {
    pub(crate) fn review_project_move(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.is_some() {
            return;
        }
        if let Some(state) = &mut self.project_move
            && matches!(state.phase, Phase::Preview(_))
        {
            state.confirming = true;
            self.focus.focus(window, cx);
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Issues, Phase};
    use crate::tests::{
        offline::{board_with_hub, offline_with, settle_board},
        render,
    };
    use gpui_kit::TestAppContext;
    use serde_json::json;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    #[gpui_kit::test]
    fn a_cancelled_preview_cannot_replace_a_new_move_session(cx: &mut TestAppContext) {
        let count = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(AtomicBool::new(false));
        let returned = Arc::new(AtomicBool::new(false));
        let (calls, held, done) = (count.clone(), release.clone(), returned.clone());
        let fixture = offline_with(move |r| move_route(r, &calls, &held, &done));
        let (board, cx) = board_with_hub(&fixture, cx);
        settle_board(&board, cx, |b| b.can_create_issue());
        board.update(cx, |b, cx| b.open_detail("SUB-2".into(), cx));
        settle_board(&board, cx, Issues::can_edit_issue);
        cx.update(|window, cx| {
            board.update(cx, |b, cx| {
                b.open_project_move(window, cx);
                b.project_move.as_mut().unwrap().destination = Some("project-OPS".into());
                b.load_move_preview(cx);
            })
        });
        settle_board(&board, cx, |_| count.load(Ordering::SeqCst) == 1);
        cx.update(|window, cx| {
            board.update(cx, |b, cx| {
                b.close_project_move(window, cx);
                b.open_project_move(window, cx);
                b.project_move.as_mut().unwrap().destination = Some("project-OPS".into());
                b.load_move_preview(cx);
            })
        });
        settle_board(
            &board,
            cx,
            |b| matches!(&b.project_move.as_ref().unwrap().phase,Phase::Preview(p) if p.issues[0].new_identifier.as_deref()==Some("OPS-8")),
        );
        release.store(true, Ordering::SeqCst);
        settle_board(&board, cx, |_| returned.load(Ordering::SeqCst));
        for _ in 0..20 {
            cx.run_until_parked();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        render(cx);
        assert!(board.read_with(cx,|b,_|matches!(&b.project_move.as_ref().unwrap().phase,Phase::Preview(p) if p.issues[0].new_identifier.as_deref()==Some("OPS-8"))));
        assert_eq!(
            fixture
                .mock
                .requests()
                .iter()
                .filter(|r| r.path.ends_with("/move"))
                .count(),
            0
        );
    }
    /// Subspace API moves an archived sub-issue with its parent and keeps
    /// it archived, so the default build neither warns nor disables Move.
    #[gpui_kit::test]
    fn an_archived_sub_issue_does_not_block_a_subspace_move(cx: &mut TestAppContext) {
        let fixture = offline_with(|r| {
            let path = r.path.split('?').next().unwrap();
            if !path.ends_with("move-preview") {
                return move_route(
                    r,
                    &AtomicUsize::new(1),
                    &AtomicBool::new(true),
                    &AtomicBool::new(false),
                );
            }
            Some((200, json!({"token":"b".repeat(64),
                "source":{"id":"project-SUB","key":"SUB","name":"Fixture"},
                "destination":{"id":"project-OPS","key":"OPS","name":"Operations"},
                "issues":[
                    {"id":"2","version":1,"oldIdentifier":"SUB-2","newIdentifier":"OPS-9","stateId":"state",
                     "stateName":"Started","cycleCleared":false,"milestoneCleared":false,"estimateCleared":false,
                     "parentDetached":false,"archived":false},
                    {"id":"3","version":1,"oldIdentifier":"SUB-3","newIdentifier":"OPS-10","stateId":"state",
                     "stateName":"Done","cycleCleared":false,"milestoneCleared":false,"estimateCleared":false,
                     "parentDetached":false,"archived":true}]}).to_string()))
        });
        let (board, cx) = board_with_hub(&fixture, cx);
        settle_board(&board, cx, |b| b.can_create_issue());
        board.update(cx, |b, cx| b.open_detail("SUB-2".into(), cx));
        settle_board(&board, cx, Issues::can_edit_issue);
        cx.update(|window, cx| {
            board.update(cx, |b, cx| {
                b.open_project_move(window, cx);
                b.project_move.as_mut().unwrap().destination = Some("project-OPS".into());
                b.load_move_preview(cx);
            })
        });
        settle_board(
            &board,
            cx,
            |b| matches!(&b.project_move.as_ref().unwrap().phase, Phase::Preview(p) if p.issues.len() == 2),
        );
        cx.update(|window, cx| {
            board.update(cx, |b, cx| {
                b.menu = None;
                b.review_project_move(window, cx);
            })
        });
        assert!(board.read_with(cx, |b, _| b.project_move.as_ref().unwrap().confirming));
        render(cx);
        assert!(crate::tests::has(cx, "issue-move-identifiers"));
        assert!(!crate::tests::has(cx, "issue-move-archived"));
        assert!(crate::tests::has(cx, "issue-move-confirm"));
        let disabled = cx.update(|window, _| {
            use gpui_kit::test::TestWindowExt as _;
            window.find("issue-move-confirm").disabled()
        });
        assert_ne!(
            disabled,
            Some(true),
            "Move stays available in the default build"
        );
    }
    fn move_route(
        r: &subspace_framework::client_test_support::Request,
        calls: &AtomicUsize,
        held: &AtomicBool,
        done: &AtomicBool,
    ) -> Option<(u16, String)> {
        let path = r.path.split('?').next().unwrap();
        if path == "/api/v1/projects" {
            return Some((200,json!({"items":[
            {"id":"project-SUB","key":"SUB","name":"Fixture","defaultStateId":"state-started"},
            {"id":"project-OPS","key":"OPS","name":"Operations","defaultStateId":"ops-started"}],"nextCursor":null}).to_string()));
        }
        if path == "/api/v1/issues/2" {
            let mut issue = crate::tests::offline::row("2", "Fixture");
            issue["description"] = json!("");
            issue["todos"] = json!([]);
            return Some((200, issue.to_string()));
        }
        if !r.path.ends_with("move-preview") {
            return None;
        }
        let index = calls.fetch_add(1, Ordering::SeqCst);
        if index == 0 {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while !held.load(Ordering::SeqCst) && std::time::Instant::now() < until {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            done.store(true, Ordering::SeqCst);
        }
        Some((200,json!({"token":"a".repeat(64),"source":{"id":"project-SUB","key":"SUB","name":"Fixture"},
                "destination":{"id":"project-OPS","key":"OPS","name":"Operations"},"issues":[{
                    "id":"2","version":1,"oldIdentifier":"SUB-2","newIdentifier":if index==0 {"OPS-7"}else{"OPS-8"},
                    "stateId":"state","stateName":"Started","cycleCleared":false,"milestoneCleared":false,
                    "estimateCleared":false,"parentDetached":false,"archived":false}]}).to_string()))
    }
}
