//! Group notes and the board-level empty state, drawn by the shared list.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyElement, Context, Entity, IntoElement as _, Render as _, Task, TestAppContext,
    VisualTestContext, Window,
};
use subspace_framework::dashboard::test_support::ToolbarTestAppContext as _;
use subspace_framework::dashboard::{CommandRun, Dashboard as _, list, prefs};

use crate::{
    Actions,
    fetch::Fetch,
    model::{Snapshot, StatusFilter},
    test_support::run,
};

fn root(board: &mut Actions, window: &mut Window, cx: &mut Context<Actions>) -> AnyElement {
    board.render(window, cx).into_any_element()
}

/// A drawn board with the Failed filter: `ann/app` loaded with one
/// successful run, `ann/lib` still loading.
fn mounted(cx: &mut TestAppContext) -> (Entity<Actions>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
    let (board, cx) = cx.mount_toolbar(900., root);
    board.update(cx, |board, cx| {
        board.repos = vec!["ann/app".into(), "ann/lib".into()];
        board.login = Some("ann".into());
        board.data.insert(
            "ann".into(),
            Snapshot {
                runs: vec![run("ann/app")],
                loaded: ["ann/app: Runs".to_owned()].into(),
                ..Default::default()
            },
        );
        board.filters.status = StatusFilter::Failed;
        board.fetch = Fetch::loading(Task::ready(()), &["ann/lib".into()]);
        board.rebuild(cx);
    });
    cx.run_until_parked();
    (board, cx)
}

#[gpui_kit::test]
fn notes_are_named_lines_that_selection_skips_and_folding_hides(cx: &mut TestAppContext) {
    let (board, cx) = mounted(cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("list-group-note-line:ann/app").label(),
            Some("ann/app: no failed runs in the latest runs")
        );
        assert_eq!(
            window.find("list-group-note-line:ann/lib").label(),
            Some("ann/lib: loading runs")
        );
        assert!(window.try_find("actions-clear-filters").is_none());
    });

    // The selection steps from header to header: a note is not a line.
    board.update(cx, |board, cx| {
        list::update(board, cx, |list| list.select_end(false));
        list::update(board, cx, |list| list.step(1));
        assert_eq!(board.list.selected().map(AsRef::as_ref), Some("ann/lib"));
        list::update(board, cx, |list| list.toggle(&"ann/app".into()));
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("list-group-note-line:ann/app").is_none(),
            "folding hides the note"
        );
        assert!(window.try_find("list-group-note-line:ann/lib").is_some());
    });
}

#[gpui_kit::test]
fn right_click_refresh_targets_the_repository_of_the_group_header(cx: &mut TestAppContext) {
    let (board, cx) = mounted(cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.right_click("list-group-count:ann/app", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let mut menu = window.within("popup-menu");
        let refresh = (0usize..12)
            .find(|index| {
                menu.try_find(*index)
                    .is_some_and(|item| item.label() == Some("Refresh repository"))
            })
            .expect("the repository menu offers Refresh repository");
        menu.click(refresh, cx);
    });
    board.read_with(cx, |board, _| {
        assert!(board.queued_repos.contains("ann/app"));
        assert!(!board.queued_repos.contains("ann/lib"));
    });
}

#[gpui_kit::test]
fn palette_refresh_targets_the_selected_repository(cx: &mut TestAppContext) {
    let (board, cx) = mounted(cx);
    let command = board.update(cx, |board, cx| {
        list::update(board, cx, |list| list.select_end(false));
        let commands = board.commands(cx);
        assert!(
            commands
                .iter()
                .any(|command| command.label == "Filter Actions…")
        );
        commands
            .into_iter()
            .find(|command| command.label == "Refresh repository")
            .expect("a selected group offers Refresh repository")
    });
    let CommandRun::Callback(refresh) = command.run else {
        panic!("Refresh repository has a callback");
    };
    cx.update(|window, cx| refresh(window, cx));
    board.read_with(cx, |board, _| {
        assert!(board.queued_repos.contains("ann/app"));
        assert!(!board.queued_repos.contains("ann/lib"));
    });
}

#[gpui_kit::test]
fn clear_filters_replaces_the_board_level_empty_state_with_runs(cx: &mut TestAppContext) {
    let (board, cx) = mounted(cx);

    board.update(cx, |board, cx| {
        board.fetch = Fetch::Idle;
        board.rebuild(cx);
        assert!(
            board.list.groups().is_empty(),
            "one board-level empty state"
        );
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
        window.click("actions-clear-filters", cx);
    });
    board.read_with(cx, |board, cx| {
        assert_eq!(board.filters.status, StatusFilter::All);
        assert_eq!(board.list.rows().count(), 1);
        assert_eq!(prefs::get(cx, "actions.status").as_deref(), Some("All"));
    });
}
