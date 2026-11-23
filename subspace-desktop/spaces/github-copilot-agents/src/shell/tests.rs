use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Context, Task as GpuiTask, TestAppContext, VisualTestContext, Window};
use subspace_framework::client_github::copilot_agents::Task;
use subspace_framework::dashboard::list::{self, ListDashboard as _, ListState};
use subspace_framework::dashboard::workspace::{SpaceDock, SpaceDocks};
use subspace_framework::dashboard::{Dashboard as _, ItemSearch, prefs};

use super::stored_docks;
use crate::CopilotAgents;
use crate::filters::{Counts, Filters};
use crate::inspector::OPEN_ID;
use crate::item::ITEM_ID;
use crate::keys::{INSPECTOR_KEY, NAVIGATOR_KEY};

const TASK_KEY: &str = "cloud:task";

/// A board that lists one running cloud task, with its docks as last
/// stored. It has no client, store, credentials or watcher, so nothing here
/// sends a request or touches a file.
fn board(_: &mut Window, cx: &mut Context<CopilotAgents>) -> CopilotAgents {
    let (navigator_open, inspector_open) = stored_docks(cx);
    let mut board = CopilotAgents {
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
        loaded_at: Some(Instant::now()),
        login: None,
        identity: None,
        navigator_open,
        inspector_open,
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
    };
    board.tasks = vec![Task {
        id: "task".into(),
        repo: "test-only/repo".into(),
        name: "Running task".into(),
        state: "in_progress".into(),
        html_url: String::new(),
        created_at: String::new(),
        updated_at: String::new(),
        pull_request: None,
        subject: None,
    }];
    board.rebuild(cx);
    board
}

fn start(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
}

/// Draws until an element that left the tree has left the last frame too.
fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
}

/// Whether the last frame drew the element `id`.
fn drawn(cx: &mut VisualTestContext, id: &'static str) -> bool {
    cx.update(|window, _| window.try_find(id).is_some())
}

#[gpui_kit::test]
fn the_navigator_draws_its_views_until_it_is_hidden(cx: &mut TestAppContext) {
    start(cx);
    let (board, cx) = cx.add_window_view(board);
    settle(cx);
    assert!(drawn(cx, "workspace-navigator"));
    assert!(drawn(cx, "github-copilot-agents-nav-view:all"));
    assert!(drawn(cx, "github-copilot-agents-nav-repo:test-only/repo"));
    assert!(drawn(cx, "github-copilot-agents-toolbar"));
    assert!(drawn(cx, "workspace-tab-strip"), "one tab names the view");

    board.update(cx, |board, cx| board.toggle_navigator(cx));
    settle(cx);
    assert!(!drawn(cx, "workspace-navigator"), "the dock is gone");
    assert!(drawn(cx, "github-copilot-agents-toolbar"), "the pane stays");
}

#[gpui_kit::test]
fn a_click_selects_a_row_and_the_inspector_shows_it_without_opening_it(cx: &mut TestAppContext) {
    start(cx);
    let (board, cx) = cx.add_window_view(board);
    settle(cx);
    assert!(
        drawn(cx, "github-copilot-agents-inspector-empty"),
        "with nothing selected the inspector asks for a selection"
    );
    board.update(cx, |board, cx| {
        let row = board.list.rows().next().cloned().unwrap();
        assert!(!board.activates_on_click(&row), "a click only selects");
        // What one click on the row does.
        list::update(board, cx, |list| list.select(TASK_KEY.into()));
        assert!(board.item.showing().is_none(), "a click opens nothing");
        assert!(board.list.open_row().is_none());
        assert_eq!(
            board.detail_key.as_deref(),
            Some(TASK_KEY),
            "the selection loads in the inspector"
        );
    });
    settle(cx);
    assert!(drawn(cx, "github-copilot-agents-peek-head"));
    assert!(drawn(cx, OPEN_ID), "the inspector offers Open");
    assert!(!drawn(cx, ITEM_ID));
    assert!(!drawn(cx, "panel-close"), "the wide detail panel is gone");

    cx.update(|window, cx| window.click("workspace-inspector-close", cx));
    settle(cx);
    assert!(!drawn(cx, "github-copilot-agents-peek-head"));
    board.read_with(cx, |board, _| {
        assert!(!board.inspector_open, "Close hides the inspector");
        assert_eq!(
            board.list.selected().map(ToString::to_string).as_deref(),
            Some(TASK_KEY),
            "the selection stays"
        );
    });
}

#[gpui_kit::test]
fn both_docks_show_until_hidden_and_a_later_board_starts_as_they_were_left(
    cx: &mut TestAppContext,
) {
    start(cx);
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            assert!(board.navigator_open && board.inspector_open);
            board.toggle_navigator(cx);
            board.toggle_inspector(cx);
            assert!(!board.navigator_open && !board.inspector_open);
            assert_eq!(prefs::get(cx, NAVIGATOR_KEY).as_deref(), Some("0"));
            assert_eq!(prefs::get(cx, INSPECTOR_KEY).as_deref(), Some("0"));
        })
        .unwrap();
    let later = cx.add_window(board);
    later
        .update(cx, |board, _, cx| {
            assert!(!board.navigator_open && !board.inspector_open);
            board.toggle_navigator(cx);
            assert!(board.navigator_open);
            assert_eq!(prefs::get(cx, NAVIGATOR_KEY).as_deref(), Some("1"));
        })
        .unwrap();
}

/// The status bar's switches read both docks and hand their clicks back
/// through the Space's dock contract.
#[gpui_kit::test]
fn the_status_bar_switches_reach_both_docks(cx: &mut TestAppContext) {
    start(cx);
    let window = cx.add_window(board);
    window
        .update(cx, |board, window, cx| {
            assert_eq!(board.docks(cx), SpaceDocks::both(true, true));
            board.toggle_dock(SpaceDock::Inspector, window, cx);
            assert_eq!(board.docks(cx), SpaceDocks::both(true, false));
            board.toggle_dock(SpaceDock::Navigator, window, cx);
            assert_eq!(board.docks(cx), SpaceDocks::both(false, false));
            assert_eq!(prefs::get(cx, NAVIGATOR_KEY).as_deref(), Some("0"));
        })
        .unwrap();
}

#[gpui_kit::test]
fn a_hidden_inspector_reads_nothing_on_selection_and_showing_it_loads_the_selection(
    cx: &mut TestAppContext,
) {
    start(cx);
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            board.toggle_inspector(cx);
            list::update(board, cx, |list| list.select(TASK_KEY.into()));
            assert!(
                board.detail_key.is_none(),
                "with the inspector hidden a selection reads nothing"
            );

            board.toggle_inspector(cx);
            assert_eq!(board.detail_key.as_deref(), Some(TASK_KEY));
            assert!(board.item.showing().is_none(), "showing it opens nothing");
            assert_eq!(prefs::get(cx, INSPECTOR_KEY).as_deref(), Some("1"));
        })
        .unwrap();
}

#[gpui_kit::test]
fn the_shell_toolbar_is_empty_and_the_status_leads_with_the_account(cx: &mut TestAppContext) {
    start(cx);
    let window = cx.add_window(board);
    window
        .update(cx, |board, window, cx| {
            assert!(
                board.render_toolbar(window, cx).is_none(),
                "the pane's toolbar holds the filters"
            );
            board.login = Some("octo-dev".into());
            let status = board.status(cx).unwrap();
            assert!(status.starts_with("GitHub · Signed in as octo-dev · 1 rows"));
        })
        .unwrap();
}

mod opened;
