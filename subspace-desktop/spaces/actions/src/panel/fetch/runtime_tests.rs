//! The run detail's reads keep only the answer they wait for, run one at a
//! time, and act on the run that shows.

use std::sync::Arc;

use gpui_kit::{Context, TestAppContext, Window};
use subspace_framework::dashboard::{Dashboard as _, list, prefs};

use super::Loaded;
use crate::graph::GraphJob;
use crate::menus::{self, Picker};
use crate::model::{RunTarget, Snapshot, View};
use crate::open::OpenTab;
use crate::panel::{LogState, RunDetail};
use crate::{Actions, test_support::run};

const FIRST: &str = "ann/app#notice#run-1";
const SECOND: &str = "ann/app#notice#run-2";

/// A board signed in as `ann` with runs 1 and 2 of `ann/app`. Its client
/// points at a closed local port, so any read it starts fails at once.
fn board(window: &mut Window, cx: &mut Context<Actions>) -> Actions {
    let mut board = Actions::new(window, cx);
    let environment =
        subspace_framework::agent_tools::test_support::environment(Default::default());
    board.client = Some(Arc::new(
        subspace_framework::client_github::Client::new(environment.credentials)
            .with_api_url("http://127.0.0.1:0"),
    ));
    board.login = Some("ann".into());
    board.repos = vec!["ann/app".into()];
    let mut second = run("ann/app");
    second.key = SECOND.into();
    if let Some(target) = second.run.as_mut() {
        target.id = 2;
    }
    board.data.insert(
        "ann".into(),
        Snapshot {
            runs: vec![run("ann/app"), second],
            ..Default::default()
        },
    );
    board.rebuild(cx);
    board
}

fn start(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
}

/// Run `id` of the fixture's workflow, at `attempt`.
fn target(id: u64, attempt: u64) -> RunTarget {
    let mut target = run("ann/app").run.expect("the fixture is a run");
    target.id = id;
    target.attempt = attempt;
    target
}

fn job(id: u64, status: &str) -> GraphJob {
    GraphJob {
        id,
        name: format!("job {id}"),
        status: status.into(),
        duration: "1s".into(),
        steps: String::new(),
    }
}

fn loaded(jobs: Vec<GraphJob>) -> Result<Loaded, String> {
    Ok(Loaded {
        jobs,
        edges: Vec::new(),
        reason: None,
    })
}

/// Run 1 open in the tab beside the view, showing, with `detail`.
fn open_first(board: &mut Actions, detail: RunDetail) {
    board.open = Some(OpenTab {
        key: FIRST.into(),
        view: View::Runs,
        label: "Run 1".into(),
        active: true,
        detail,
    });
}

fn job_ids(detail: &RunDetail) -> Vec<u64> {
    detail.jobs.iter().map(|job| job.id).collect()
}

#[gpui_kit::test]
fn quick_selection_reads_one_run_at_a_time_and_skips_the_runs_passed(cx: &mut TestAppContext) {
    start(cx);
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            board.load_detail(target(1, 1), cx);
            let first = board.detail.request;
            assert_eq!(board.reading, Some(first));
            board.load_detail(target(2, 1), cx);
            board.load_detail(target(3, 1), cx);
            assert_eq!(board.reading, Some(first), "no read starts while one runs");
            assert_eq!(board.detail.target, Some(target(3, 1)));
            let last = board.detail.request;

            let generation = board.generation;
            board.finish_detail(generation, first, loaded(vec![job(7, "failure")]), cx);
            assert!(
                board.detail.jobs.is_empty(),
                "run 1's jobs never show for run 3"
            );
            assert!(board.detail.loading);
            assert_eq!(
                board.reading,
                Some(last),
                "the last selection is read next, and run 2 never"
            );
        })
        .unwrap();
}

#[gpui_kit::test]
fn only_the_newest_log_read_lands_and_a_failed_log_keeps_the_jobs(cx: &mut TestAppContext) {
    start(cx);
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            open_first(
                board,
                RunDetail {
                    target: Some(target(1, 1)),
                    jobs: vec![job(7, "failure"), job(8, "failure")],
                    log: LogState::Loading,
                    log_request: 12,
                    ..RunDetail::default()
                },
            );
            let generation = board.generation;
            board.finish_job_log(generation, FIRST, 12, Ok("job 8's log".into()), cx);
            board.finish_job_log(generation, FIRST, 11, Ok("job 7's log".into()), cx);
            let open = board.open.as_ref().expect("the tab stays open");
            assert_eq!(open.detail.log, LogState::Loaded("job 8's log".into()));

            if let Some(open) = board.open.as_mut() {
                open.detail.log = LogState::Loading;
                open.detail.log_request = 13;
            }
            board.finish_job_log(generation, FIRST, 13, Err("log expired".into()), cx);
            let open = board.open.as_ref().expect("the tab stays open");
            assert_eq!(open.detail.log, LogState::Failed("log expired".into()));
            assert_eq!(job_ids(&open.detail), [7, 8], "the jobs stay");
            assert!(
                open.detail.error.is_none(),
                "a log that failed is not a jobs failure"
            );
        })
        .unwrap();
}

#[gpui_kit::test]
fn a_new_attempt_starts_the_opened_tab_afresh_and_drops_the_old_attempts_answer(
    cx: &mut TestAppContext,
) {
    start(cx);
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            board.detail = RunDetail {
                target: Some(target(1, 1)),
                jobs: vec![job(7, "failure")],
                request: 5,
                ..RunDetail::default()
            };
            open_first(
                board,
                RunDetail {
                    target: Some(target(1, 1)),
                    jobs: vec![job(7, "failure")],
                    log: LogState::Loaded("attempt 1's log".into()),
                    request: 5,
                    ..RunDetail::default()
                },
            );
            board.load_detail(target(1, 2), cx);
            let open = &board.open.as_ref().expect("the tab stays open").detail;
            assert_eq!(open.target, Some(target(1, 2)));
            assert!(open.loading && open.jobs.is_empty());
            assert_eq!(open.log, LogState::None, "attempt 1's log is gone");

            let generation = board.generation;
            board.finish_detail(generation, 5, loaded(vec![job(7, "failure")]), cx);
            let open = &board.open.as_ref().expect("the tab stays open").detail;
            assert!(open.jobs.is_empty(), "attempt 1's late answer is dropped");

            let current = open.request;
            board.finish_detail(generation, current, loaded(vec![job(9, "success")]), cx);
            let open = &board.open.as_ref().expect("the tab stays open").detail;
            assert_eq!(job_ids(open), [9]);
            assert_eq!(job_ids(&board.detail), [9], "the selection shares the read");
        })
        .unwrap();
}

#[gpui_kit::test]
fn a_rerun_seen_while_another_run_is_selected_reads_the_opened_run_again(cx: &mut TestAppContext) {
    start(cx);
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            open_first(
                board,
                RunDetail {
                    target: Some(target(1, 1)),
                    jobs: vec![job(7, "failure")],
                    log: LogState::Loaded("attempt 1's log".into()),
                    ..RunDetail::default()
                },
            );
            board.detail = RunDetail {
                target: Some(target(2, 1)),
                ..RunDetail::default()
            };
            if let Some(data) = board.data.get_mut("ann")
                && let Some(first) = data.runs[0].run.as_mut()
            {
                first.attempt = 2;
            }
            board.refresh_open_detail(cx);
            let open = &board.open.as_ref().expect("the tab stays open").detail;
            assert_eq!(open.target, Some(target(1, 2)));
            assert!(open.loading && open.jobs.is_empty());
            assert_eq!(open.log, LogState::None);
            assert_eq!(board.reading, Some(open.request));
        })
        .unwrap();
}

#[gpui_kit::test]
fn the_failed_job_log_picker_offers_and_reads_only_the_selected_runs_jobs(cx: &mut TestAppContext) {
    start(cx);
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            board.detail = RunDetail {
                target: Some(target(1, 1)),
                jobs: vec![job(7, "failure"), job(8, "success")],
                ..RunDetail::default()
            };
            board.list.select(SECOND.into());
            assert!(menus::picker_values(board, Picker::Log).is_empty());
            board.load_job_log(7, cx);
            assert!(board.open.is_none(), "run 1's job is not read for run 2");

            board.list.select(FIRST.into());
            assert_eq!(menus::picker_values(board, Picker::Log), ["7 · job 7"]);
            board.load_job_log(7, cx);
            let open = board.open.as_ref().expect("the log opens run 1's tab");
            assert_eq!(open.key, FIRST);
            assert_eq!(open.detail.log, LogState::Loading);
            assert_eq!(job_ids(&open.detail), [7, 8]);
        })
        .unwrap();
}

#[gpui_kit::test]
fn the_failed_job_log_picker_offers_no_job_of_an_earlier_attempt(cx: &mut TestAppContext) {
    start(cx);
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            board.detail = RunDetail {
                target: Some(target(1, 1)),
                jobs: vec![job(7, "failure")],
                ..RunDetail::default()
            };
            board.list.select(FIRST.into());
            // A refresh moves run 1 on to attempt 2 before its jobs are read:
            // attempt 1's failed job is not offered.
            if let Some(data) = board.data.get_mut("ann")
                && let Some(first) = data.runs[0].run.as_mut()
            {
                first.attempt = 2;
            }
            assert!(menus::picker_values(board, Picker::Log).is_empty());
            board.load_job_log(7, cx);
            assert!(
                board.open.is_none(),
                "attempt 1's log is not read into attempt 2's tab"
            );
        })
        .unwrap();
}

#[gpui_kit::test]
fn the_writes_act_on_the_run_the_opened_tab_shows(cx: &mut TestAppContext) {
    start(cx);
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            let key = |record: Option<&crate::model::Record>| record.map(|r| r.key.clone());
            list::update(board, cx, |list| list.select(FIRST.into()));
            board.open_tab(FIRST, cx);
            board.select_tab(0, cx);
            list::update(board, cx, |list| list.select(SECOND.into()));
            assert_eq!(key(board.write_target()), Some(SECOND.to_owned()));

            board.select_tab(1, cx);
            assert_eq!(board.list.selected().map(AsRef::as_ref), Some(FIRST));
            assert_eq!(
                key(board.write_target()),
                Some(FIRST.to_owned()),
                "showing run 1 selects it again"
            );

            list::update(board, cx, |list| list.select(SECOND.into()));
            assert!(board.opened().is_none(), "choosing run 2 shows the list");

            board.select_tab(1, cx);
            board.list.select(SECOND.into());
            assert!(
                board.write_target().is_none(),
                "no write acts on a run out of sight"
            );
        })
        .unwrap();
}
