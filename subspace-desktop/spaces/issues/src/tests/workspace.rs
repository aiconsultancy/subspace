//! The Space in the shared workspace: the navigator's rows, the dock
//! switches, the inspector's peek, and the click that selects rather than
//! opens.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_api_client::hub::IssueStatus;
use subspace_framework::dashboard::prefs;
use subspace_framework::dashboard::workspace::SpaceDock;

use super::offline::{board_with_hub, offline, settle_board};
use super::{
    choice, docks, has, label, listed, listed_at, press, render, requests, selected,
    use_dock_switch,
};
use crate::Issues;
use crate::model::{SubIssues, Tab};
use crate::view::lanes::{OPEN_ROW_ID, lead_id};

fn click(cx: &mut VisualTestContext, id: &str) {
    let id = id.to_owned();
    cx.update(|window, cx| window.click(id, cx));
    render(cx);
}

fn is_loaded(board: &Issues, identifier: &str) -> bool {
    board.issue_view.snapshot.as_ref().is_some_and(|issue| {
        issue.identifier() == identifier && *issue.status() == IssueStatus::Loaded
    })
}

fn saved(cx: &mut VisualTestContext, key: &str) -> Option<String> {
    cx.update(|_, cx| prefs::get(cx, key))
}

#[gpui_kit::test]
fn navigator_rows_choose_the_view_and_show_the_current_one(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    requests(&board, cx);
    click(cx, "issues-nav-view:backlog");
    cx.update(|_, cx| assert_eq!(board.read(cx).tab, Tab::Backlog));
    assert_eq!(
        requests(&board, cx),
        [choice(Tab::Backlog, "", SubIssues::Hidden)]
    );
    assert_eq!(
        label(cx, "workspace-navigator-title").as_deref(),
        Some("Issues")
    );
    click(cx, "issues-nav-view:activity");
    cx.update(|_, cx| assert!(board.read(cx).activity.visible));
    assert!(!has(cx, "issues-peek"), "the feed has no peek");
    click(cx, "issues-nav-view:all");
    cx.update(|_, cx| {
        let board = board.read(cx);
        assert!(!board.activity.visible);
        assert_eq!(board.tab, Tab::All);
    });
}

#[gpui_kit::test]
fn hiding_the_navigator_puts_the_view_menu_and_pickers_in_the_toolbar(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    assert!(has(cx, "workspace-navigator"));
    assert!(!has(cx, "issues-view-button"));
    assert_eq!(docks(&board, cx).navigator, Some(true));
    use_dock_switch(&board, cx, SpaceDock::Navigator);
    assert!(!has(cx, "workspace-navigator"));
    assert_eq!(docks(&board, cx).navigator, Some(false));
    assert_eq!(
        label(cx, "issues-view-button").as_deref(),
        Some("View: Active")
    );
    assert!(has(cx, "issues-project-button"));
    cx.update(|window, _| {
        let picker = window.find("issues-project-button").bounds();
        let refresh = window.find("issues-refresh").bounds();
        let trailing = window.find("workspace-toolbar-trailing").bounds();
        assert!(picker.right() <= refresh.left());
        assert_eq!(refresh.right(), trailing.right());
    });
    assert_eq!(saved(cx, "issues.navigator").as_deref(), Some("hidden"));
    use_dock_switch(&board, cx, SpaceDock::Navigator);
    assert!(has(cx, "workspace-navigator"));
    assert!(!has(cx, "issues-view-button"));
    assert_eq!(saved(cx, "issues.navigator").as_deref(), Some("shown"));
}

#[gpui_kit::test]
fn closing_the_inspector_hides_the_peek_until_its_switch_brings_it_back(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    assert!(has(cx, "issues-peek"));
    assert!(has(cx, "issues-peek-empty"), "nothing selected yet");
    click(cx, "workspace-inspector-close");
    assert!(!has(cx, "issues-peek"));
    assert_eq!(saved(cx, "issues.inspector").as_deref(), Some("hidden"));
    assert_eq!(docks(&board, cx).inspector, Some(false));
    use_dock_switch(&board, cx, SpaceDock::Inspector);
    assert!(has(cx, "issues-peek"));
    assert_eq!(saved(cx, "issues.inspector").as_deref(), Some("shown"));
}

#[gpui_kit::test]
fn a_narrow_window_holds_the_inspector_back_without_changing_the_choice(cx: &mut TestAppContext) {
    let (board, cx) = listed_at(cx, 900.);
    assert!(has(cx, "workspace-navigator"));
    assert!(!has(cx, "issues-peek"));
    use_dock_switch(&board, cx, SpaceDock::Inspector);
    assert!(!has(cx, "issues-peek"));
    cx.update(|_, cx| assert!(board.read(cx).docks.inspector, "still chosen"));
    assert_eq!(saved(cx, "issues.inspector"), None);
}

#[gpui_kit::test]
fn keys_select_a_row_into_the_peek_and_only_the_selected_row_offers_open(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    assert!(!has(cx, OPEN_ROW_ID));
    press(cx, "j");
    press(cx, "j");
    assert_eq!(selected(&board, cx).as_deref(), Some("issue:id-SUB-955"));
    assert!(has(cx, OPEN_ROW_ID));
    assert_eq!(
        label(cx, "workspace-inspector-title").as_deref(),
        Some("SUB-955")
    );
    assert_eq!(
        label(cx, "issues-peek-state").as_deref(),
        Some("In Progress")
    );
    for id in [
        "issues-peek-open",
        "inspector-fact:Priority",
        "inspector-fact:Assignee",
        "inspector-fact:Project",
    ] {
        assert!(has(cx, id), "{id}");
    }
    assert!(!has(cx, "issues-peek-empty"));
}

#[gpui_kit::test]
fn a_click_selects_and_a_double_click_or_open_opens_the_page(cx: &mut TestAppContext) {
    let fixture = offline();
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, |board| board.issue("issue:1").is_some());
    render(cx);
    click(cx, &lead_id("issue:1"));
    assert_eq!(selected(&board, cx).as_deref(), Some("issue:1"));
    cx.update(|_, cx| {
        assert!(
            board.read(cx).issue_view.snapshot.is_none(),
            "one click only selects"
        )
    });
    assert_eq!(
        label(cx, "workspace-inspector-title").as_deref(),
        Some("SUB-1")
    );
    click(cx, OPEN_ROW_ID);
    settle_board(&board, cx, |board| is_loaded(board, "SUB-1"));
    render(cx);
    assert!(has(cx, "issues-detail"));
    back_to_list(&board, cx);
    assert!(has(cx, "issues-tabs"));
    let row = lead_id("issue:1");
    cx.update(|window, cx| window.double_click(row, cx));
    settle_board(&board, cx, |board| is_loaded(board, "SUB-1"));
    render(cx);
    assert!(has(cx, "issues-detail"));
}

#[gpui_kit::test]
fn the_issue_page_keeps_its_properties_in_the_inspector_or_after_the_body(cx: &mut TestAppContext) {
    let fixture = offline();
    let (board, cx) = board_with_hub(&fixture, cx);
    board.update(cx, |board, cx| board.open_detail("SUB-1".into(), cx));
    settle_board(&board, cx, |board| {
        !board.projects.is_empty()
            && board
                .issue_view
                .snapshot
                .as_ref()
                .is_some_and(|issue| issue.detail().is_some())
    });
    render(cx);
    assert!(has(cx, "issue-tabs"));
    assert!(!has(cx, "issues-tabs"));
    assert!(has(cx, "workspace-navigator"));
    assert_eq!(
        label(cx, "workspace-inspector-title").as_deref(),
        Some("Properties")
    );
    cx.update(|window, _| {
        let inspector = window.find("issue-inspector").bounds();
        let properties = window.find("issue-properties").bounds();
        let main = window.find("issue-main-column").bounds();
        assert!(properties.left() >= inspector.left());
        assert!(main.right() <= inspector.left());
        assert!(
            main.size.width
                <= gpui_kit::px(subspace_framework::dashboard::workspace::READING_WIDTH_PX)
        );
    });
    use_dock_switch(&board, cx, SpaceDock::Inspector);
    assert!(!has(cx, "issue-inspector"));
    cx.update(|window, _| {
        let main = window.find("issue-main-column").bounds();
        let side = window.find("issue-side-column").bounds();
        assert!(side.top() >= main.bottom(), "they end the reading column");
        assert_eq!(side.left(), main.left());
    });
    for id in ["issue-properties", "issue-relations", "issue-record"] {
        assert!(has(cx, id), "{id}");
    }
}

fn back_to_list(board: &Entity<Issues>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| board.update(cx, |board, cx| board.back_to_list(window, cx)));
    render(cx);
}
