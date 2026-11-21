use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;
use subspace_api_client::initiatives::Catalog;

use super::{connected, has, mount, render, requests};
use crate::docks::Dock;
use crate::fixtures::project;
use crate::model::ProjectScope;

fn selected_initiative(shared: &Entity<Catalog>, cx: &mut VisualTestContext) -> Option<String> {
    shared.read_with(cx, |catalog, _| {
        catalog.selected().map(|initiative| initiative.id.clone())
    })
}

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
    // The navigator lists the initiative's projects and every initiative.
    for id in [
        "issues-nav-project:OPS",
        "issues-nav-project:RPT",
        "issues-nav-initiative:ops",
        "issues-nav-initiative:design",
        "issues-nav-initiative:local",
    ] {
        assert!(has(cx, id), "{id}");
    }
    assert!(!has(cx, "issues-nav-project:SUB"));
    assert!(!has(cx, "issues-nav-project:*"));
    cx.update(|window, cx| window.click("issues-nav-initiative:design", cx));
    render(cx);
    assert_eq!(selected_initiative(&shared, cx).as_deref(), Some("design"));
    board.read_with(cx, |board, _| {
        assert_eq!(board.project.key(), "SUB");
        assert_eq!(project_keys(board), ["SUB"]);
    });

    // With the navigator hidden, the toolbar's pickers stand in for it.
    board.update(cx, |board, cx| board.toggle_dock(Dock::Navigator, cx));
    render(cx);
    assert!(!has(cx, "workspace-navigator"));
    cx.update(|window, cx| {
        let initiative = window.find("issues-initiative-button");
        assert_eq!(initiative.label(), Some("Initiative: Design"));
        assert!(
            initiative.bounds().right() <= window.find("issues-project-button").bounds().left()
        );
        window.click("issues-initiative-button", cx);
    });
    render(cx);
    cx.simulate_keystrokes("o p e r a t i o n s enter");
    render(cx);
    assert_eq!(selected_initiative(&shared, cx).as_deref(), Some("ops"));
    board.read_with(cx, |board, _| {
        assert_eq!(project_keys(board), ["OPS", "RPT"]);
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

#[gpui_kit::test]
fn a_nett_initiative_under_all_teams_shows_its_rows(cx: &mut TestAppContext) {
    // NETT keeps the list to an initiative by filter, with any team or All
    // teams chosen. The hub names no team for All teams.
    crate::initiative::initiatives_by_filter_for_test(Some(true));
    let (board, _shared, cx) = setup(cx);
    board.update(cx, |board, cx| {
        board.set_project(ProjectScope::AllProjects, cx);
        board.initiative_id = Some("ops".into());
        board.listing.content = None;
        let view = crate::sync::HubView {
            profile_name: "Fixture".into(),
            access: connected(),
            projects: board.projects.clone(),
            groups: Some(Vec::new()),
            list_project: None,
            read_at: None,
            list: None,
            request_revision: 0,
            status: crate::listing::Status::Idle,
            hidden_subissues_count: None,
            my_issues_count: None,
            live: subspace_api_client::hub::LiveStatus::Off,
        };
        board.apply_view(view, cx);
        assert!(
            board.listing.content.is_some(),
            "the initiative's rows are shown, not taken for an earlier choice"
        );
    });
    crate::initiative::initiatives_by_filter_for_test(None);
}
