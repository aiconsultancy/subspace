use crate::{
    CopilotAgents,
    model::{AgentRow, Kind},
};
use gpui_kit::{Context, TestAppContext, Window};
use std::sync::Arc;
use std::{cell::RefCell, rc::Rc};
use subspace_framework::dashboard::chat_sessions::{ChatSession, ChatSessions};
use subspace_framework::dashboard::{ChatRequest, DashboardEvent};
use subspace_framework::dashboard::{Dashboard as _, prefs, refresh::Preferences};

fn board(window: &mut Window, cx: &mut Context<CopilotAgents>) -> CopilotAgents {
    let mut board = CopilotAgents::new(window, cx);
    let environment =
        subspace_framework::agent_tools::test_support::environment(Default::default());
    board.client = Some(Arc::new(
        subspace_framework::client_github::Client::new(environment.credentials)
            .with_api_url("http://127.0.0.1:0"),
    ));
    board.store = None;
    board.all_rows = vec![AgentRow {
        key: "cloud:task".into(),
        kind: Kind::Cloud,
        id: "task".into(),
        title: "Running task".into(),
        repo: "ann/app".into(),
        state: "in_progress".into(),
        created_at: String::new(),
        updated_at: String::new(),
        monitor: true,
        subject: None,
        html_url: String::new(),
        pull_request: None,
    }];
    board
}

#[gpui_kit::test]
fn copilot_agents_off_suppresses_watcher_list_reads_but_explicit_load_works(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
        Preferences {
            enabled: false,
            interval_seconds: None,
        }
        .save("github-copilot-agents", cx);
    });
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            board.poll(cx);
            assert!(
                !board.loading,
                "Off must stop the task watcher's list request"
            );
            board.load_all(cx);
            assert!(board.loading, "manual load must remain available while Off");
        })
        .unwrap();
}

#[gpui_kit::test]
fn copilot_agents_off_suppresses_shell_refresh_and_reenables_without_restart(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            Preferences {
                enabled: false,
                interval_seconds: None,
            }
            .save("github-copilot-agents", cx);
            board.background_refresh(cx);
            assert!(!board.loading);
            Preferences::default().save("github-copilot-agents", cx);
            board.background_refresh(cx);
            assert!(board.loading);
        })
        .unwrap();
}

#[gpui_kit::test]
fn session_snapshot_observation_rebuilds_local_rows_and_activation_requests_refresh(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
    let window = cx.add_window(board);
    let entity = window.update(cx, |_, _, cx| cx.entity()).unwrap();
    let requests = Rc::new(RefCell::new(Vec::new()));
    let recorded = requests.clone();
    let _subscription = cx.update(|cx| {
        cx.subscribe(&entity, move |_, event, _| {
            if let DashboardEvent::Chat(request) = event {
                recorded.borrow_mut().push(request.clone());
            }
        })
    });
    for (open, busy, state) in [
        (true, true, crate::model::LOCAL_BUSY),
        (true, false, crate::model::LOCAL_OPEN),
        (false, false, crate::model::LOCAL_SAVED),
    ] {
        cx.update(|cx| {
            cx.set_global(ChatSessions(vec![ChatSession {
                id: "saved-chat".into(),
                title: "Saved conversation".into(),
                modified: "now".into(),
                modified_at: "2026-11-04T00:00:00Z".into(),
                created_at: "2026-11-03T00:00:00Z".into(),
                open,
                busy,
            }]))
        });
        window
            .update(cx, |board, _, _| {
                let row = board
                    .all_rows
                    .iter()
                    .find(|row| row.id == "saved-chat")
                    .unwrap();
                assert_eq!(row.kind, Kind::Local);
                assert_eq!(row.state, state);
                assert_eq!(row.title, "Saved conversation");
            })
            .unwrap();
    }
    window
        .update(cx, |board, window, cx| board.activate(window, cx))
        .unwrap();
    assert!(matches!(
        requests.borrow().as_slice(),
        [ChatRequest::ListSessions]
    ));
}
