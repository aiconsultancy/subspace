use std::{collections::HashMap, rc::Rc};

use gpui_kit::{Context, Task as GpuiTask, TestAppContext, Window};
use subspace_framework::client_github::copilot_agents::{Task, TaskDetail};
use subspace_framework::core::http::Error;
use subspace_framework::dashboard::{ItemSearch, list::ListState, prefs};

use crate::{
    CopilotAgents,
    filters::{Counts, Filters},
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
