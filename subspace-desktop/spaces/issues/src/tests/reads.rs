//! What the board asks Subspace API for, and when (spec sections 3 and 7): the
//! visible list, a folded group once unfolded, the next page near a
//! group's end, a changed project, activation after a minute and Reload.

use std::time::{Duration, Instant};

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use subspace_framework::dashboard::{Dashboard as _, ListDashboard as _};

use super::{choice, connected, design_groups, listed, mount, press, render, requests, selected};
use crate::fixtures::{issue, project};
use crate::listing::More;
use crate::model::{ProjectScope, SubIssues, Tab};
use crate::requests::Request;
use crate::view::lanes::more_retry_id;

#[gpui_kit::test]
fn a_ready_profile_reads_the_active_list_with_the_default_project(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    board.update(cx, |board, cx| {
        board.set_projects(vec![project("SUB", "Subspace", false)], cx);
        board.set_access(connected(), cx);
    });
    assert_eq!(
        requests(&board, cx),
        [choice(Tab::Active, "", SubIssues::Hidden)]
    );
}

#[gpui_kit::test]
fn unfolding_done_reads_its_rows_and_remembers_the_fold(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    requests(&board, cx);
    board.update(cx, |board, cx| {
        subspace_framework::dashboard::list::update(board, cx, |list| {
            list.select("group:state-done".into())
        });
    });
    press(cx, "enter");
    cx.update(|_, cx| {
        let board = board.read(cx);
        assert!(!board.list().is_folded("group:state-done"));
        assert_eq!(board.folds.unfolded, ["state-done"]);
        assert_eq!(
            subspace_framework::dashboard::prefs::get_list(cx, "issues.unfolded"),
            ["state-done"]
        );
    });
    assert_eq!(
        requests(&board, cx),
        [Request::Group {
            group: "state-done".into()
        }]
    );
    board.update(cx, |board, cx| {
        let mut groups = design_groups();
        groups[2].rows = vec![issue(
            "SUB-90",
            "Sample shipped work",
            ("Done", "completed"),
            serde_json::json!({}),
        )];
        groups[2].more = More::None;
        board.show_groups(groups, true, cx);
    });
    render(cx);
    assert_eq!(
        selected(&board, cx).as_deref(),
        Some("group:state-done"),
        "the selection stays on the header"
    );
}

#[gpui_kit::test]
fn selecting_near_a_groups_end_reads_its_next_page(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    requests(&board, cx);
    board.update(cx, |board, cx| {
        subspace_framework::dashboard::list::update(board, cx, |list| {
            list.select("issue:id-SUB-1141".into())
        });
    });
    assert_eq!(
        requests(&board, cx),
        [Request::More {
            group: "state-todo".into()
        }]
    );
    board.update(cx, |board, cx| {
        let mut groups = design_groups();
        groups[1].more = More::Failed;
        groups[2].more = More::None;
        board.show_groups(groups, true, cx);
    });
    render(cx);
    cx.update(|_, cx| {
        let board = board.read(cx);
        let more = board
            .list()
            .rows()
            .find(|row| row.key.as_ref() == "more:state-todo");
        assert_eq!(
            more.map(|row| row.title.as_ref()),
            Some("Couldn't load more")
        );
    });
    assert!(cx.update(|window, _| window.try_find(more_retry_id("state-todo")).is_some()));
    board.update(cx, |board, cx| board.read_more("state-todo", cx));
    assert_eq!(
        requests(&board, cx),
        [Request::More {
            group: "state-todo".into()
        }],
        "Retry reads the page again"
    );
}

#[gpui_kit::test]
fn loaded_projects_name_the_project_and_an_archived_choice_gives_way(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    requests(&board, cx);
    board.update(cx, |board, cx| {
        board.set_projects(
            vec![
                project("SUB", "Subspace API", false),
                project("OPS", "Operations", false),
            ],
            cx,
        );
    });
    render(cx);
    assert!(requests(&board, cx).is_empty(), "the project stands");
    assert_eq!(
        cx.update(|window, _| window
            .find("issues-project-button")
            .label()
            .map(str::to_owned))
            .as_deref(),
        Some("Project: SUB · Subspace API")
    );
    board.update(cx, |board, cx| {
        board.set_project(ProjectScope::Project("OPS".into()), cx)
    });
    requests(&board, cx);
    cx.update(|_, cx| {
        assert_eq!(
            subspace_framework::dashboard::prefs::get(cx, "issues.project").as_deref(),
            Some("OPS")
        );
    });
    board.update(cx, |board, cx| {
        board.set_projects(
            vec![
                project("SUB", "Subspace API", false),
                project("OPS", "Operations", true),
            ],
            cx,
        );
    });
    assert_eq!(
        requests(&board, cx),
        [choice(Tab::Active, "", SubIssues::Hidden)],
        "an archived project gives way to the only active project"
    );
}

#[gpui_kit::test]
fn activation_reads_again_only_after_a_minute(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    requests(&board, cx);
    cx.update(|window, cx| board.update(cx, |board, cx| board.activate(window, cx)));
    assert!(requests(&board, cx).is_empty(), "read moments ago");
    board.update(cx, |board, _| {
        if let Some(content) = board.listing.content.as_mut() {
            content.read_at = Instant::now() - Duration::from_secs(61);
        }
    });
    cx.update(|window, cx| board.update(cx, |board, cx| board.activate(window, cx)));
    assert_eq!(
        requests(&board, cx),
        [choice(Tab::Active, "", SubIssues::Hidden)]
    );
}

#[gpui_kit::test]
fn reload_reads_the_visible_list_and_keeps_its_rows(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    requests(&board, cx);
    cx.update(|window, cx| board.update(cx, |board, cx| board.reload(window, cx)));
    let Request::List(choice) = choice(Tab::Active, "", SubIssues::Hidden) else {
        unreachable!("choice builds a list read");
    };
    assert_eq!(
        requests(&board, cx),
        [Request::Reload(choice)],
        "Reload reads the projects too"
    );
    cx.update(|_, cx| {
        let board = board.read(cx);
        assert!(board.busy(cx));
        assert!(board.listing.content.is_some(), "Reload keeps the rows");
    });
}
