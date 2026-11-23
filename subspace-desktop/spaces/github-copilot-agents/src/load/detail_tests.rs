use std::{collections::HashMap, rc::Rc};

use gpui_kit::{Context, Task as GpuiTask, TestAppContext, Window};
use subspace_framework::client_github::copilot_agent_sessions::SessionLog;
use subspace_framework::client_github::copilot_agents::{Task, TaskDetail};
use subspace_framework::core::http::Error;
use subspace_framework::dashboard::{ItemSearch, list::ListState, prefs};

use crate::{
    CopilotAgents,
    filters::{Counts, Filters},
    load::WorkIdentity,
    logs::{self, LogState, OpenLog},
};

// No system store, credentials, watcher or filesystem paths are used by this fixture.
fn board(_: &mut Window, cx: &mut Context<CopilotAgents>) -> CopilotAgents {
    CopilotAgents {
        focus: cx.focus_handle(),
        client: None,
        store: None,
        list: ListState::default(),
        tasks: Vec::new(),
        saved: Vec::new(),
        known: HashMap::new(),
        all_rows: Vec::new(),
        rows: Vec::new(),
        counts: Counts::default(),
        filters: Filters::default(),
        detail: None,
        detail_key: None,
        detail_error: None,
        detail_request: Default::default(),
        log: None,
        log_request: 0,
        loading: false,
        error: None,
        loaded_at: None,
        login: None,
        identity: None,
        navigator_open: true,
        inspector_open: true,
        item: Default::default(),
        previous: HashMap::new(),
        backoff: Default::default(),
        window_active: false,
        menu: None,
        menu_anchors: Default::default(),
        search: Rc::new(ItemSearch::new([])),
        _watcher: GpuiTask::ready(()),
        _subscriptions: Vec::new(),
        overflow_toolbar: None,
    }
}

fn detail(name: &str) -> Result<TaskDetail, Error> {
    Ok(TaskDetail {
        task: Task {
            id: "test-only-task".into(),
            repo: "test-only/repo".into(),
            name: name.into(),
            state: "completed".into(),
            html_url: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
            pull_request: None,
            subject: None,
        },
        sessions: Vec::new(),
    })
}

#[gpui_kit::test]
fn newer_detail_survives_an_older_same_target_completion(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            board.detail_key = Some("cloud:task".into());
            let older = board.detail_request.begin("cloud:task".into());
            let newer = board.detail_request.begin("cloud:task".into());
            board.apply_detail(newer, detail("newer"), cx);
            board.apply_detail(older, detail("older"), cx);
            assert_eq!(board.detail.as_ref().unwrap().task.name, "newer");
            assert_eq!(board.known["test-only-task"].name, "newer");
            assert!(board.detail_error.is_none());
        })
        .unwrap();
}

#[gpui_kit::test]
fn leaving_a_detail_target_rejects_its_pending_answer(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            board.detail_key = Some("cloud:task".into());
            let pending = board.detail_request.begin("cloud:task".into());
            board.load_detail("local:chat", cx);
            board.apply_detail(pending, detail("old target"), cx);
            assert_eq!(board.detail_key.as_deref(), Some("local:chat"));
            assert!(board.detail.is_none());
            assert!(board.known.is_empty());
            assert!(board.detail_error.is_none());
        })
        .unwrap();
}

fn identity(login: &str, account_id: &str) -> WorkIdentity {
    WorkIdentity {
        login: login.into(),
        account_id: account_id.into(),
    }
}

fn start(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
}

/// Every list that loads names the account it was read as, so a credential
/// changed during the session shows in the navigator and the status bar.
#[gpui_kit::test]
fn each_loaded_list_renames_the_account_and_a_failed_one_keeps_it(cx: &mut TestAppContext) {
    start(cx);
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            board.login = Some("first-login".into());
            board.apply_list(
                (
                    Ok((Vec::new(), identity("second-login", "U_second"))),
                    Vec::new(),
                ),
                cx,
            );
            assert_eq!(board.login.as_deref(), Some("second-login"));
            assert_eq!(board.identity, Some(identity("second-login", "U_second")));

            let offline = Error::Network("offline".into());
            board.apply_list((Err(offline), Vec::new()), cx);
            assert_eq!(
                board.login.as_deref(),
                Some("second-login"),
                "the list on show was read as this account"
            );
            assert_eq!(board.identity, Some(identity("second-login", "U_second")));
        })
        .unwrap();
}

fn open_log(state: LogState) -> OpenLog {
    OpenLog {
        task_key: "cloud:test-only-task".into(),
        session_id: "session-1".into(),
        cache_key: logs::cache_key("session-1", "2026-10-01T09:00:00Z"),
        state,
    }
}

fn session_log() -> SessionLog {
    SessionLog {
        entries: Vec::new(),
        truncated: false,
    }
}

/// A list read as another account drops the log the first account read.
#[gpui_kit::test]
fn a_list_read_as_another_account_closes_the_open_log(cx: &mut TestAppContext) {
    start(cx);
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            board.identity = Some(identity("octo-dev", "U_first"));
            board.log = Some(open_log(LogState::Ready(session_log())));
            board.apply_list(
                (
                    Ok((Vec::new(), identity("octo-dev", "U_first"))),
                    Vec::new(),
                ),
                cx,
            );
            assert!(board.log.is_some(), "the same account keeps its log");
            board.apply_list(
                (
                    Ok((Vec::new(), identity("octo-other", "U_other"))),
                    Vec::new(),
                ),
                cx,
            );
            assert!(board.log.is_none(), "another account's list closes it");
        })
        .unwrap();
}

/// A log read as another account than the one the Space shows is not
/// shown, even when it answers the newest request: it reads as
/// unavailable instead. A log read as the Space's account is shown.
#[gpui_kit::test]
fn a_log_read_as_another_account_than_the_space_shows_is_not_shown(cx: &mut TestAppContext) {
    start(cx);
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            board.identity = Some(identity("octo-dev", "U_first"));
            board.log = Some(open_log(LogState::Loading));
            board.log_request = 1;
            board.apply_log(
                1,
                Ok((session_log(), false, identity("octo-other", "U_other"))),
                cx,
            );
            assert_eq!(
                board.log.as_ref().map(|log| log.state.clone()),
                Some(LogState::Unavailable)
            );

            board.log = Some(open_log(LogState::Loading));
            board.log_request = 2;
            board.apply_log(
                2,
                Ok((session_log(), false, identity("octo-dev", "U_first"))),
                cx,
            );
            assert_eq!(
                board.log.as_ref().map(|log| log.state.clone()),
                Some(LogState::Ready(session_log()))
            );
        })
        .unwrap();
}
