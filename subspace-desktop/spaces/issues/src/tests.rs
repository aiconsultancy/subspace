//! Render, keyboard and focus tests for the board. The pure modules keep
//! their own tests beside them, under `tests/`.

mod activity;
mod authoring;
mod board_view;
mod decisions;
mod detail;
mod editing;
mod first_pages;
mod http_details;
mod initiative;
mod issue_panels;
mod keyboard;
mod list_filters;
mod live_reread;
pub(crate) mod offline;
mod reads;
mod search;
mod storage;
mod timeline;
mod workspace;

use gpui_kit::test::{TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{
    AppContext as _, Bounds, Entity, TestAppContext, VisualTestContext, WindowBounds,
    WindowOptions, px, size,
};
use serde_json::json;
use subspace_framework::dashboard::workspace::{SpaceDock, SpaceDocks};
use subspace_framework::dashboard::{Dashboard as _, ListDashboard as _};

use crate::Issues;
use crate::fixtures::{issue, person};
use crate::listing::{IssueGroup, More};
use crate::model::{ProjectScope, StateKind, SubIssues, Tab};
use crate::requests::{ListChoice, Request};
use crate::scope::Access;
use crate::view::lanes::lead_id;

/// The design's capture size.
const WIDTH: f32 = 1440.;
const HEIGHT: f32 = 900.;

/// The board in a 1440 × 900 window, with the shared and board keys bound.
pub(crate) fn mount(cx: &mut TestAppContext) -> (Entity<Issues>, &mut VisualTestContext) {
    mount_at(cx, WIDTH)
}

pub(crate) fn mount_at(
    cx: &mut TestAppContext,
    width: f32,
) -> (Entity<Issues>, &mut VisualTestContext) {
    mount_with_tab(cx, width, Some(Tab::Active))
}

pub(crate) fn mount_with_tab(
    cx: &mut TestAppContext,
    width: f32,
    tab: Option<Tab>,
) -> (Entity<Issues>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        subspace_framework::dashboard::prefs::init_file(None, cx);
        if let Some(tab) = tab {
            subspace_framework::dashboard::prefs::set(cx, "issues.tab", tab.key());
        }
        subspace_framework::dashboard::bind_keys(cx);
        Issues::bind_keys(cx);
    });
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            Default::default(),
            size(px(width), px(HEIGHT)),
        ))),
        ..Default::default()
    };
    let (window, board) = cx.update(|cx| {
        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| Issues::new(window, cx))
        })
        .unwrap()
    });
    let cx = VisualTestContext::from_window(window, cx).into_mut();
    cx.update(|window, cx| {
        window.activate_window();
        board.update(cx, |board, cx| {
            board.focus.focus(window, cx);
            board.activate(window, cx);
        });
        window.render_frame(cx);
    });
    (board, cx)
}

pub(crate) fn render(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
}

/// Delivers frames as the display would: the window's next-frame callbacks,
/// then a draw, so work waiting for frames runs.
pub(crate) fn settle_frames(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
        render(cx);
    }
}

/// Uses a dock's switch as the shell's status bar does: through the Space's
/// dock contract.
pub(crate) fn use_dock_switch(board: &Entity<Issues>, cx: &mut VisualTestContext, dock: SpaceDock) {
    cx.update(|window, cx| {
        board.update(cx, |board, cx| {
            subspace_framework::dashboard::Dashboard::toggle_dock(board, dock, window, cx)
        })
    });
    render(cx);
}

/// What the Space reports for the status bar's dock switches.
pub(crate) fn docks(board: &Entity<Issues>, cx: &mut VisualTestContext) -> SpaceDocks {
    cx.update(|_, cx| board.read(cx).docks(cx))
}

/// A synthetic connected identity.
pub(crate) fn connected() -> Access {
    Access::Ready {
        display_name: "Ada Fixture".into(),
    }
}

/// The design's `list` fixture: In Progress and Todo on the Active tab,
/// and a folded Done group.
pub(crate) fn design_groups() -> Vec<IssueGroup> {
    let tracker = json!({
        "type": "tracker",
        "priority": 0,
        "progress": {"done": 31, "total": 52, "canceled": 0, "duplicate": 0, "sample": [], "hasMore": true},
        "project": {"id": "project-1", "name": "Sample Tracking"},
        "assignee": person("Jo Fixture"),
    });
    let labelled = json!({
        "priority": 1,
        "labels": [
            {"id": "label-1", "name": "Needs owner", "color": "#f2994a", "state": "active"},
            {"id": "label-2", "name": "Feature", "color": "#4cb782", "state": "active"},
        ],
        "project": {"id": "project-2", "name": "Pilot Go Live"},
        "dueDate": "2026-12-17",
        "assignee": person("Jo Fixture"),
    });
    vec![
        IssueGroup {
            index: 0,
            key: "state-started".into(),
            name: "In Progress".into(),
            kind: StateKind::Started,
            total: Some(2),
            rows: vec![
                issue(
                    "SUB-955",
                    "Tracker: Feature module — Sample Tracking",
                    ("In Progress", "started"),
                    tracker,
                ),
                issue(
                    "SUB-1540",
                    "Day 1 blocker: confirmed sample booking refuses timing and fee amendments",
                    ("In Progress", "started"),
                    json!({"priority": 4}),
                ),
            ],
            more: More::None,
        },
        IssueGroup {
            index: 1,
            key: "state-todo".into(),
            name: "Todo".into(),
            kind: StateKind::Unstarted,
            total: Some(30),
            rows: vec![issue(
                "SUB-1141",
                "Design Contacts and link people when they join the sample org",
                ("Todo", "unstarted"),
                labelled,
            )],
            more: More::Available,
        },
        IssueGroup {
            index: 2,
            key: "state-done".into(),
            name: "Done".into(),
            kind: StateKind::Completed,
            total: Some(12),
            rows: Vec::new(),
            more: More::Unread,
        },
    ]
}

/// The board signed in, with the design's list read.
pub(crate) fn listed(cx: &mut TestAppContext) -> (Entity<Issues>, &mut VisualTestContext) {
    listed_at(cx, WIDTH)
}

/// The design's list in a window `width` wide.
pub(crate) fn listed_at(
    cx: &mut TestAppContext,
    width: f32,
) -> (Entity<Issues>, &mut VisualTestContext) {
    let (board, cx) = mount_at(cx, width);
    board.update(cx, |board, cx| {
        board.set_projects(vec![crate::fixtures::project("SUB", "Subspace", false)], cx);
        board.set_access(connected(), cx);
        board.set_project(ProjectScope::Project("SUB".into()), cx);
        board.show_groups(design_groups(), true, cx);
    });
    render(cx);
    (board, cx)
}

pub(crate) fn has(cx: &mut VisualTestContext, id: &'static str) -> bool {
    cx.update(|window, _| window.try_find(id).is_some())
}

/// The accessible name of the element `id`.
pub(crate) fn label(cx: &mut VisualTestContext, id: &'static str) -> Option<String> {
    cx.update(|window, _| {
        window
            .try_find(id)
            .and_then(|found| found.label().map(str::to_owned))
    })
}

/// Whether an element is named `text` for assistive technology.
pub(crate) fn labelled(cx: &mut VisualTestContext, text: &str) -> bool {
    cx.update(|window, _| window.try_find_by_label(text).is_some())
}

/// Whether the row with list key `key` is drawn.
pub(crate) fn row_shown(cx: &mut VisualTestContext, key: &str) -> bool {
    cx.update(|window, _| window.try_find(lead_id(key)).is_some())
}

pub(crate) fn press(cx: &mut VisualTestContext, key: &str) {
    cx.update(|window, cx| window.press(key, cx));
    render(cx);
}

/// The requests the board made since the last call.
pub(crate) fn requests(board: &Entity<Issues>, cx: &mut VisualTestContext) -> Vec<Request> {
    board.update(cx, |board, _| board.take_requests())
}

pub(crate) fn selected(board: &Entity<Issues>, cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| board.read(cx).list().selected().map(ToString::to_string))
}

/// A list read for NET with these choices.
pub(crate) fn choice(tab: Tab, search: &str, sub_issues: SubIssues) -> Request {
    Request::List(ListChoice {
        tab,
        project: ProjectScope::Project("SUB".into()),
        search: search.into(),
        sub_issues,
        filters: Default::default(),
        initiative: None,
    })
}

/// The accessible name of the row with list key `key`.
pub(crate) fn row_name(
    board: &Entity<Issues>,
    key: &str,
    cx: &mut VisualTestContext,
) -> Option<String> {
    cx.update(|_, cx| {
        let board = board.read(cx);
        let row = board.list().rows().find(|row| row.key.as_ref() == key)?;
        Some(board.row_accessible_label(row, cx).to_string())
    })
}
