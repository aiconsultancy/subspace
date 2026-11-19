use crate::{
    Actions,
    dialogs::Confirmation,
    fetch::Fetch,
    load::mapping,
    model::{RunTarget, Snapshot},
    writes::Operation,
};
use gpui_kit::{Context, Task, TestAppContext, Window};
use std::sync::Arc;
use subspace_framework::dashboard::{Dashboard as _, prefs, refresh::Preferences};

fn board(window: &mut Window, cx: &mut Context<Actions>) -> Actions {
    let mut board = Actions::new(window, cx);
    let environment =
        subspace_framework::agent_tools::test_support::environment(Default::default());
    board.client = Some(Arc::new(
        subspace_framework::client_github::Client::new(environment.credentials)
            .with_api_url("http://127.0.0.1:0"),
    ));
    board.visible = true;
    board.repos = vec!["ann/app".into()];
    board.login = Some("ann".into());
    let mut run = mapping::notice("ann/app", "run", "CI");
    run.active = true;
    run.run = Some(RunTarget {
        repo: "ann/app".into(),
        id: 1,
        workflow_id: 2,
        workflow: "CI".into(),
        path: String::new(),
        sha: String::new(),
        branch: "main".into(),
        actor: "ann".into(),
        attempt: 1,
    });
    board.data.insert(
        "ann".into(),
        Snapshot {
            runs: vec![run],
            ..Default::default()
        },
    );
    board
}

#[gpui_kit::test]
fn visible_actions_off_skips_poll_requests_but_manual_reload_works(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
        Preferences {
            enabled: false,
            interval_seconds: None,
        }
        .save("actions", cx);
    });
    let window = cx.add_window(board);
    window
        .update(cx, |board, window, cx| {
            super::poll_active(board, cx);
            assert!(board.fetch.is_idle());
            board.reload(window, cx);
            assert!(matches!(board.fetch, Fetch::Loading { .. }));
        })
        .unwrap();
}

#[gpui_kit::test]
fn saved_actions_toggle_takes_effect_on_the_existing_board(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            Preferences {
                enabled: false,
                interval_seconds: None,
            }
            .save("actions", cx);
            super::poll_active(board, cx);
            assert!(board.fetch.is_idle());
            Preferences::default().save("actions", cx);
            super::poll_active(board, cx);
            assert!(matches!(board.fetch, Fetch::Refreshing { .. }));
        })
        .unwrap();
}

#[gpui_kit::test]
fn a_write_during_a_full_load_keeps_the_load_and_refreshes_the_run_after_it(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
    let window = cx.add_window(board);
    window
        .update(cx, |board, window, cx| {
            board.load_all(cx);
            assert!(matches!(board.fetch, Fetch::Loading { .. }));
            let record = board.data["ann"].runs[0].clone();
            let target = record.run.clone();
            board.pending = Some(Confirmation::new(Operation::Rerun, record, window, cx));
            board.write_finished(board.generation, Ok(()), cx);
            assert!(board.pending.is_none());
            let Fetch::Loading { follow_up, .. } = &board.fetch else {
                panic!("the write replaced the full load");
            };
            assert!(follow_up.first() == target.as_ref());
            board.apply_results(board.generation, &board.repos.clone(), Vec::new(), cx);
            assert!(matches!(board.fetch, Fetch::Refreshing { .. }));
        })
        .unwrap();
}

#[gpui_kit::test]
fn a_requested_repository_refresh_waits_for_polling_and_coalesces(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            board.repos.push("ann/lib".into());
            board.fetch = Fetch::refreshing(Task::ready(()));
            board.refresh_repo("ann/lib".into(), cx);
            board.refresh_repo("ann/lib".into(), cx);
            assert_eq!(board.queued_repos.len(), 1);

            super::apply_updates(board, board.generation, Vec::new(), cx);
            assert!(board.fetch.loads_repo("ann/lib"));
            assert!(!board.fetch.loads_repo("ann/app"));
            assert!(board.queued_repos.is_empty());
            assert!(board.last_full_load_started_at.is_none());
            assert_eq!(board.records().len(), 1, "other repositories stay visible");

            board.refresh_repo("ann/lib".into(), cx);
            assert!(board.queued_repos.is_empty(), "already loading");
        })
        .unwrap();
}

#[gpui_kit::test]
fn results_without_an_account_end_the_fetch(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            board.load_all(cx);
            board.login = None;
            board.apply_results(board.generation, &board.repos.clone(), Vec::new(), cx);
            assert!(board.fetch.is_idle());
            assert!(!board.busy(cx));
            board.login = Some("ann".into());
            let target = board.data["ann"].runs[0].run.clone().into_iter().collect();
            super::refresh_runs(board, target, cx);
            assert!(matches!(board.fetch, Fetch::Refreshing { .. }));
            board.login = None;
            super::apply_updates(board, board.generation, Vec::new(), cx);
            assert!(board.fetch.is_idle());
            assert!(!board.busy(cx));
        })
        .unwrap();
}
