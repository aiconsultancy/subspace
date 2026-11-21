//! The list, the navigator, the toolbar and the peek.

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_api_client::Error;
use subspace_framework::dashboard::workspace::{SpaceDock, SpaceDocks};
use subspace_framework::dashboard::{Dashboard as _, ListDashboard as _, prefs};

use crate::tests::{
    click, click_row, has, label, labelled, mount, mount_with, render, rows, selected,
};
use crate::{Decisions, fixtures, search};

#[gpui_kit::test]
fn the_first_read_groups_decisions_and_folds_the_settled_ones(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    let groups: Vec<String> = cx.update(|_, cx| {
        board
            .read(cx)
            .list()
            .groups()
            .iter()
            .map(|group| group.name.to_string())
            .collect()
    });
    assert_eq!(groups, ["Proposed", "Accepted", "Superseded", "Rejected"]);
    assert_eq!(
        rows(&board, cx),
        [
            "decision:d1",
            "decision:d2",
            "decision:d3",
            "decision:d4",
            "decision:d5"
        ],
        "Superseded and Rejected start folded"
    );
    let status = cx.update(|_, cx| board.read(cx).status(cx)).unwrap();
    assert!(status.starts_with("Subspace API · Signed in as local-operator · 8 decisions"));
    assert_eq!(
        label(cx, "decisions-account").as_deref(),
        Some("Signed in as local-operator")
    );
    assert!(has(cx, "decisions-view:accepted"));
    assert!(has(cx, "decisions-project:project:WARP"));
}

#[gpui_kit::test]
fn one_click_selects_a_decision_and_peeks_without_opening_it(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    assert!(has(cx, "decisions-inspector-empty"), "nothing selected yet");
    click_row(cx, "d2");
    assert_eq!(selected(&board, cx).as_deref(), Some("decision:d2"));
    assert!(!has(cx, "decisions-page"), "a click does not open it");
    assert_eq!(
        label(cx, "decisions-peek-head").as_deref(),
        Some("Accepted, Read the relay position after the correction is applied")
    );
    assert_eq!(
        label(cx, "inspector-fact:Made by").as_deref(),
        Some("Made by: aurora-lyra · captain · fixture-model (agent)")
    );
    assert_eq!(
        label(cx, "inspector-fact:Recorded by").as_deref(),
        Some("Recorded by: local-operator")
    );
    assert_eq!(
        label(cx, "decisions-related-row:d6").as_deref(),
        Some("Supersedes: Read the relay position before each burn")
    );
    assert!(has(cx, "decisions-issue-row:NAV-2"));
    assert!(has(cx, "decisions-issue-row:NAV-5"));
    assert!(has(cx, "decisions-open"));
}

#[gpui_kit::test]
fn a_supersession_row_selects_the_other_decision_and_unfolds_its_group(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    click_row(cx, "d2");
    click(cx, "decisions-related:d6");
    assert_eq!(selected(&board, cx).as_deref(), Some("decision:d6"));
    assert!(rows(&board, cx).contains(&"decision:d6".to_owned()));
    assert!(!has(cx, "decisions-page"));
}

#[gpui_kit::test]
fn a_navigator_view_reads_that_status_and_is_remembered(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    click(cx, "decisions-view:superseded");
    assert_eq!(rows(&board, cx), ["decision:d6", "decision:d7"]);
    assert_eq!(
        cx.update(|_, cx| prefs::get(cx, "decisions.view"))
            .as_deref(),
        Some("superseded")
    );
    click(cx, "decisions-maker:people");
    assert!(
        rows(&board, cx).is_empty(),
        "no person made a superseded one"
    );
    let (title, _) = cx.update(|_, cx| board.read(cx).empty_message());
    assert_eq!(title.as_ref(), "No decisions match these choices");
    click(cx, "decisions-view:all");
    assert_eq!(rows(&board, cx), ["decision:d3", "decision:d5"]);
}

#[gpui_kit::test]
fn a_project_row_reads_that_project(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    click(cx, "decisions-project:project:WARP");
    assert_eq!(rows(&board, cx), ["decision:d1", "decision:d4"]);
    assert_eq!(
        cx.update(|_, cx| prefs::get(cx, "decisions.project"))
            .as_deref(),
        Some("project:WARP")
    );
}

#[gpui_kit::test]
fn the_search_reads_after_it_is_committed_and_opens_every_group(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    cx.update(|window, cx| {
        board.update(cx, |board, cx| {
            board
                .search
                .update(cx, |input, cx| input.set_value("relay", window, cx));
            search::commit(board, cx);
        });
    });
    render(cx);
    assert_eq!(
        rows(&board, cx),
        ["decision:d2", "decision:d6"],
        "a search unfolds the groups it may have found something in"
    );
}

#[gpui_kit::test]
fn a_failed_search_shows_no_rows_found_for_the_previous_text(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    board.update(cx, |board, _| {
        board.reader = fixtures::failing(Error::Network)
    });
    cx.update(|window, cx| {
        board.update(cx, |board, cx| {
            board
                .search
                .update(cx, |input, cx| input.set_value("relay", window, cx));
            search::commit(board, cx);
        });
    });
    render(cx);
    assert!(
        rows(&board, cx).is_empty(),
        "the previous text's rows do not stay under the new text"
    );
    assert_eq!(label(cx, "decisions-state").as_deref(), Some("Offline"));
}

/// Uses a dock's switch as the shell's status bar does: through the Space's
/// dock contract.
fn use_dock_switch(board: &Entity<Decisions>, cx: &mut VisualTestContext, dock: SpaceDock) {
    cx.update(|window, cx| board.update(cx, |board, cx| board.toggle_dock(dock, window, cx)));
    render(cx);
}

#[gpui_kit::test]
fn hiding_the_navigator_puts_the_views_in_a_toolbar_menu(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    assert!(has(cx, "workspace-navigator"));
    let docks = cx.update(|_, cx| board.read(cx).docks(cx));
    assert_eq!(
        docks,
        SpaceDocks::both(true, true),
        "both status bar switches"
    );
    use_dock_switch(&board, cx, SpaceDock::Navigator);
    assert!(!has(cx, "workspace-navigator"));
    assert!(cx.update(|_, cx| !board.read(cx).docks.navigator));
    assert!(labelled(cx, "View: All decisions"));
    use_dock_switch(&board, cx, SpaceDock::Inspector);
    assert!(!has(cx, "decisions-inspector-empty"));
    assert_eq!(
        cx.update(|_, cx| prefs::get(cx, "decisions.inspector"))
            .as_deref(),
        Some("hidden")
    );
}

#[gpui_kit::test]
fn a_failed_first_read_says_why_and_offers_retry(cx: &mut TestAppContext) {
    let (board, cx) = mount_with(fixtures::failing(Error::SignInNeeded), cx);
    assert_eq!(
        label(cx, "decisions-state").as_deref(),
        Some("Sign-in required")
    );
    assert!(rows(&board, cx).is_empty());
    let status = cx.update(|_, cx| board.read(cx).status(cx)).unwrap();
    assert_eq!(status.as_ref(), "Subspace API · Sign-in required");
    board.update(cx, |board, _| board.reader = fixtures::reader());
    click(cx, "decisions-retry");
    assert!(!has(cx, "decisions-state"));
    assert_eq!(rows(&board, cx).len(), 5);
}
