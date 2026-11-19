use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;
use subspace_api_client::initiatives::Catalog;

use super::{connected, mount, render, requests};
use crate::fixtures::project;
use crate::model::ProjectScope;

#[gpui_kit::test]
fn shared_selection_limits_projects_and_the_issues_picker_updates_the_session(
    cx: &mut TestAppContext,
) {
    let (board, shared, cx) = setup(cx);

    // Simulate the same change as choosing Operations in Channels.
    shared.update(cx, |catalog, cx| catalog.pick("ops", cx));
    render(cx);
    board.read_with(cx, |board, _| {
        assert_eq!(board.project.key(), "RPT");
        assert_eq!(project_keys(board), ["OPS", "RPT"]);
    });
    cx.update(|window, cx| {
        let initiative = window.find("issues-initiative-button");
        assert_eq!(initiative.label(), Some("Initiative: Operations"));
        assert!(
            initiative.bounds().right() <= window.find("issues-project-button").bounds().left()
        );
        window.click("issues-initiative-button", cx);
    });
    render(cx);
    cx.simulate_keystrokes("d e s i g n enter");
    render(cx);
    shared.read_with(cx, |catalog, _| {
        assert_eq!(
            catalog.selected().map(|initiative| initiative.id.as_str()),
            Some("design")
        );
    });
    board.read_with(cx, |board, _| {
        assert_eq!(board.project.key(), "SUB");
        assert_eq!(project_keys(board), ["SUB"]);
    });
}

#[gpui_kit::test]
fn empty_initiative_does_not_query_every_project_and_can_be_cleared(cx: &mut TestAppContext) {
    let (board, shared, cx) = setup(cx);
    requests(&board, cx);
    shared.update(cx, |catalog, cx| catalog.pick("empty", cx));
    render(cx);
    assert!(
        requests(&board, cx).is_empty(),
        "an empty initiative does not query every project"
    );
    board.read_with(cx, |board, _| {
        assert!(crate::initiative::no_initiative_projects(board))
    });

    shared.update(cx, |catalog, cx| catalog.pick("local", cx));
    render(cx);
    board.read_with(cx, |board, _| {
        assert_eq!(project_keys(board), ["SUB", "OPS", "RPT", "*"]);
        assert_eq!(board.project.key(), "RPT", "the saved project is restored");
    });
}

#[gpui_kit::test]
fn empty_activity_reloads_the_catalogue_without_requesting_a_global_feed(cx: &mut TestAppContext) {
    let (board, shared, cx) = setup(cx);
    shared.update(cx, |catalog, cx| catalog.pick("empty", cx));
    render(cx);
    requests(&board, cx);
    cx.update(|window, cx| {
        board.update(cx, |board, cx| {
            board.open_activity(window, cx);
            board.read_activity(true, cx);
            assert!(!board.activity.busy());
        });
    });
    assert_eq!(
        requests(&board, cx),
        [crate::requests::Request::ReloadProjects]
    );
}

#[gpui_kit::test]
fn activity_reload_reconnects_after_the_actor_is_retired(cx: &mut TestAppContext) {
    let (board, _, cx) = setup(cx);
    board.update(cx, |board, cx| {
        use crate::{listing::Failure, scope::Access};
        board.set_access(Access::Failed(Failure::Forbidden), cx);
        board.activity.visible = true;
        board.take_requests();
        board.retry(cx);
    });
    assert!(matches!(
        requests(&board, cx).as_slice(),
        [
            crate::requests::Request::ReloadProjects,
            crate::requests::Request::Activity { reload: true, .. }
        ]
    ));
}

fn setup(
    cx: &mut TestAppContext,
) -> (
    Entity<crate::Issues>,
    Entity<Catalog>,
    &mut VisualTestContext,
) {
    let (board, cx) = mount(cx);
    let shared = cx.update(|_, cx| Catalog::shared(cx));
    shared.update(cx, |catalog, cx| {
        let items = serde_json::from_value(json!([
            {"id":"design", "name":"Design", "channels":"#design"},
            {"id":"ops", "name":"Operations", "channels":"#ops"},
            {"id":"empty", "name":"Empty initiative", "channels":""}
        ]))
        .unwrap();
        catalog.set_memberships_for_test(Some("ada".into()), Ok(items), cx);
    });
    board.update(cx, |board, cx| {
        let mut sub = project("SUB", "Subspace", false);
        sub.initiative_id = Some("design".into());
        let mut ops = project("OPS", "Operations", false);
        ops.initiative_id = Some("ops".into());
        let mut reports = project("RPT", "Reports", false);
        reports.initiative_id = Some("ops".into());
        let mut archived = project("OLD", "Archived", true);
        archived.initiative_id = Some("ops".into());
        board.set_projects(vec![sub, ops, reports, archived], cx);
        board.set_access(connected(), cx);
        board.set_project(ProjectScope::Project("RPT".into()), cx);
    });

    (board, shared, cx)
}

fn project_keys(board: &crate::Issues) -> Vec<String> {
    crate::pickers::scoped_options(board)
        .map(|(scope, _)| scope.key().to_owned())
        .collect()
}
