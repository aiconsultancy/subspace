//! Slice 5 exercises the real toolbar controls and their hub query.

mod counts;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext, px};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::workspace::text_edge;

use super::{docks, has, labelled, listed, press, render};
use crate::Issues;

fn click(cx: &mut VisualTestContext, id: &'static str) {
    cx.update(|window, cx| window.click(id, cx));
    render(cx);
}

fn pick(cx: &mut VisualTestContext, text: &str) {
    cx.update(|window, cx| window.input(text, cx));
    render(cx);
    press(cx, "enter");
}

fn parameters(board: &Entity<Issues>, cx: &mut VisualTestContext) -> Vec<(String, String)> {
    cx.update(|_, cx| {
        board
            .read(cx)
            .choice()
            .query()
            .grouped_parameters()
            .unwrap()
    })
}

#[gpui_kit::test]
fn assignee_menu_applies_and_clears_a_filter(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    click(cx, "issues-assignee-button");
    pick(cx, "No assignee");
    assert!(parameters(&board, cx).contains(&("assignee".into(), "none".into())));
    click(cx, "issues-assignee-button");
    pick(cx, "All assignees");
    assert!(
        !parameters(&board, cx)
            .iter()
            .any(|(key, _)| key == "assignee")
    );
    press(cx, "1");
    click(cx, "issues-assignee-button");
    assert!(
        !labelled(cx, "No assignee"),
        "My issues remains assigned to me"
    );
    assert!(parameters(&board, cx).contains(&("assignee".into(), "me".into())));
    pick(cx, "All assignees");
    assert_eq!(
        cx.update(|_, cx| board.read(cx).tab),
        crate::model::Tab::All
    );
    assert!(
        !parameters(&board, cx)
            .iter()
            .any(|(key, _)| key == "assignee")
    );
}

#[gpui_kit::test]
fn priority_menu_toggles_multiple_values_and_restores_focus(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    click(cx, "issues-priority-button");
    press(cx, "2");
    press(cx, "3");
    let query = parameters(&board, cx);
    assert!(query.contains(&("priority".into(), "1".into())));
    assert!(query.contains(&("priority".into(), "2".into())));
    press(cx, "2");
    assert!(!parameters(&board, cx).contains(&("priority".into(), "1".into())));
    press(cx, "escape");
    assert!(!has(cx, "menu"));
    press(cx, "4");
    assert_eq!(
        cx.update(|_, cx| board.read(cx).tab),
        crate::model::Tab::All
    );
}

#[gpui_kit::test]
fn filters_menu_combines_status_type_due_and_archived(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    board.update(cx, |board, _| {
        board.projects = vec![serde_json::from_value(serde_json::json!({
            "id": "project-NET", "key": "SUB", "name": "Subspace API", "defaultStateId": "state-started",
            "states": [{"id": "state-started", "projectId": "project-NET", "name": "In Progress", "type": "started", "position": 1, "isDefault": true, "issueCount": 2}]
        })).unwrap()];
        board.projects[0].states = vec![serde_json::from_value(serde_json::json!({"id":"state-started", "projectId":"project-SUB", "name":"In Progress", "type":"started", "position":1, "isDefault":true})).unwrap()];
    });
    for (text, pair) in [
        ("In Progress", ("state", "state-started")),
        ("Trackers", ("type", "tracker")),
        ("Overdue", ("due", "overdue")),
        ("Show archived", ("includeArchived", "true")),
    ] {
        click(cx, "issues-filters-button");
        pick(cx, text);
        assert!(parameters(&board, cx).contains(&(pair.0.into(), pair.1.into())));
        press(cx, "escape");
    }
    board.update(cx, |board, cx| {
        board.set_project(crate::model::ProjectScope::AllProjects, cx)
    });
    click(cx, "issues-filters-button");
    pick(cx, "In progress");
    assert!(parameters(&board, cx).contains(&("stateType".into(), "started".into())));
    assert!(!parameters(&board, cx).iter().any(|(key, _)| key == "state"));
}

#[gpui_kit::test]
fn group_counts_follow_the_name_and_the_list_fills_the_pane(cx: &mut TestAppContext) {
    use subspace_framework::dashboard::ListDashboard as _;

    let (board, cx) = listed(cx);
    cx.update(|window, cx| {
        let pane = window.find("workspace-pane").bounds();
        let navigator = window.find("workspace-navigator").bounds();
        let peek = window.find("issues-peek").bounds();
        assert!(navigator.right() <= pane.left() && pane.right() <= peek.left());
        let count = window.find("list-group-count:group:state-started").bounds();
        assert!(count.left() > pane.left() && count.right() < pane.left() + px(300.));
        assert_eq!(
            board.read(cx).list_side_inset(),
            Some(text_edge(&Tokens::current(cx))),
            "rows use the shared text edge, not an inset of the Space's own"
        );
    });
}

#[gpui_kit::test]
fn list_fidelity_options_preserve_the_activity_layout(cx: &mut TestAppContext) {
    use subspace_framework::dashboard::ListDashboard as _;

    let (board, cx) = listed(cx);
    let edge = cx.update(|_, cx| text_edge(&Tokens::current(cx)));
    cx.update(|_, cx| {
        assert_eq!(board.read(cx).list_side_inset(), Some(edge));
        assert!(board.read(cx).group_count_after_name());
        assert!(board.read(cx).comfortable_rows());
    });
    board.update(cx, |board, _| board.activity.visible = true);
    cx.update(|_, cx| {
        assert_eq!(board.read(cx).list_side_inset(), Some(edge));
        assert!(!board.read(cx).group_count_after_name());
        assert!(!board.read(cx).comfortable_rows());
    });
    render(cx);
    assert!(has(cx, "issues-tabs"));
    assert!(has(cx, "workspace-navigator"));
    assert!(!has(cx, "issues-search"), "the feed has no search");
    assert!(!has(cx, "issues-peek"), "the feed has no peek");
    assert_eq!(
        docks(&board, cx).inspector,
        None,
        "no inspector switch in the status bar"
    );
}

#[gpui_kit::test]
fn my_issues_assignee_does_not_leak_into_other_tabs(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    for key in ["2", "3"] {
        press(cx, "1");
        click(cx, "issues-assignee-button");
        pick(cx, "Me");
        press(cx, key);
        assert!(
            !parameters(&board, cx)
                .iter()
                .any(|(key, _)| key == "assignee"),
            "My issues supplies its own assignee predicate"
        );
        assert_eq!(
            cx.update(|_, cx| board.read(cx).filters.assignee.clone()),
            None
        );
    }
}

#[gpui_kit::test]
fn empty_filtered_lists_describe_the_filters(cx: &mut TestAppContext) {
    use subspace_framework::dashboard::ListDashboard as _;

    let (board, cx) = listed(cx);
    for choice in [
        "assignee:me",
        "priority:1",
        "state:started",
        "stateType:started",
        "type:tracker",
        "due:overdue",
        "archived",
    ] {
        for query in ["", "sample"] {
            board.update(cx, |board, cx| {
                board.filters = Default::default();
                board.filters.pick(choice);
                board.query = query.into();
                board.show_groups(Vec::new(), true, cx);
            });
            render(cx);
            cx.update(|_, cx| {
                let (title, message) = board.read(cx).empty_message();
                assert_eq!(
                    title.as_ref(),
                    "No issues match your filters",
                    "{choice}, {query}"
                );
                assert_eq!(message.as_ref(), "Try changing your filters.");
            });
        }
    }
    board.update(cx, |board, _| board.filters = Default::default());
    cx.update(|_, cx| {
        assert_eq!(
            board.read(cx).empty_message().0.as_ref(),
            "No issues match your search"
        );
    });
    board.update(cx, |board, _| board.query.clear());
    cx.update(|_, cx| {
        assert_eq!(
            board.read(cx).empty_message().0.as_ref(),
            "No active issues"
        );
    });
}
