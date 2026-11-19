//! A full load shows each repository as it arrives.

use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::{Context, SharedString, TestAppContext, Window};
use subspace_framework::dashboard::list::GroupNote;
use subspace_framework::dashboard::{Dashboard as _, prefs, refresh::Preferences};

use super::RepoResult;
use crate::{Actions, fetch::Fetch, test_support::run};

fn board(window: &mut Window, cx: &mut Context<Actions>) -> Actions {
    let mut board = Actions::new(window, cx);
    let environment =
        subspace_framework::agent_tools::test_support::environment(Default::default());
    board.client = Some(Arc::new(
        subspace_framework::client_github::Client::new(environment.credentials)
            .with_api_url("http://127.0.0.1:0"),
    ));
    board.visible = true;
    board.repos = vec!["ann/app".into(), "ann/lib".into()];
    board.login = Some("ann".into());
    board
}

fn result(repo: &str, runs: Vec<crate::model::Record>) -> RepoResult {
    RepoResult {
        repo: repo.into(),
        runs: Ok(runs),
        deployments: Ok(Vec::new()),
        releases: Ok(Vec::new()),
    }
}

#[gpui_kit::test]
fn hidden_actions_discovers_an_account_when_automatic_updates_are_enabled(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            board.visible = false;
            board.login = None;
            Preferences {
                enabled: false,
                interval_seconds: None,
            }
            .save(Actions::ID, cx);
            board.background_refresh(cx);
            assert!(board.fetch.is_idle());

            Preferences::default().save(Actions::ID, cx);
            board.background_refresh(cx);
            assert!(matches!(board.fetch, Fetch::Discovering { .. }));
            assert!(board.last_full_load_started_at.is_some());
            assert!(!board.visible && board.poll.is_none());
        })
        .unwrap();
}

#[gpui_kit::test]
fn active_run_updates_do_not_delay_the_next_full_refresh(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            board.visible = false;
            board.loaded_at = Some(Instant::now());
            board.last_full_load_started_at = Some(Instant::now() - Duration::from_secs(540));
            board.background_refresh(cx);
            assert!(board.fetch.is_idle());

            board.last_full_load_started_at = Some(Instant::now() - Duration::from_secs(600));
            board.background_refresh(cx);
            assert!(board.fetch.loads_repo("ann/app"));
            assert!(board.fetch.loads_repo("ann/lib"));
            assert!(!board.full_load_due(cx));
        })
        .unwrap();
}

#[gpui_kit::test]
fn a_repository_refresh_keeps_the_board_wide_freshness(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            let last_full_load = Instant::now() - Duration::from_secs(60);
            board.loaded_at = Some(last_full_load);
            let repo = "ann/app".to_owned();
            board.refresh_repo(repo.clone(), cx);
            assert!(board.fetch.loads_repo(&repo));
            board.apply_results(board.generation, &[repo], Vec::new(), cx);
            assert_eq!(board.loaded_at, Some(last_full_load));
        })
        .unwrap();
}

#[gpui_kit::test]
fn each_repository_stops_loading_when_its_records_arrive(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            board.load_all(cx);
            let loading = GroupNote::Loading("Loading runs…".into());
            assert_eq!(
                board.notes.get(&SharedString::from("ann/app")),
                Some(&loading)
            );
            assert_eq!(
                board.notes.get(&SharedString::from("ann/lib")),
                Some(&loading)
            );
            assert!(board.status_text().ends_with("· Loading runs…"));

            board.apply_repo(board.generation, result("ann/app", Vec::new()), cx);
            assert_eq!(
                board.notes.get(&SharedString::from("ann/app")),
                Some(&GroupNote::Empty("No workflow runs".into()))
            );
            assert_eq!(
                board.notes.get(&SharedString::from("ann/lib")),
                Some(&loading)
            );
            assert!(matches!(board.fetch, Fetch::Loading { .. }));

            board.apply_repo(
                board.generation,
                result("ann/lib", vec![run("ann/lib")]),
                cx,
            );
            assert!(
                !board.notes.contains_key(&SharedString::from("ann/lib")),
                "rows, no note"
            );
            assert!(board.status_text().ends_with("· Loading runs…"));

            board.apply_results(board.generation, &board.repos.clone(), Vec::new(), cx);
            assert!(board.fetch.is_idle());
            assert!(board.status_text().ends_with("s ago"));
        })
        .unwrap();
}

#[gpui_kit::test]
fn a_late_repository_from_another_account_is_ignored(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            board.load_all(cx);
            let stale = board.generation;
            board.switch_account("bob".into(), cx);
            board.apply_repo(stale, result("ann/app", vec![run("ann/app")]), cx);
            assert!(!board.data.contains_key("ann"));
            assert!(
                board
                    .data
                    .get("bob")
                    .is_none_or(|data| data.runs.is_empty())
            );
        })
        .unwrap();
}

#[gpui_kit::test]
fn a_failed_repository_shows_the_banner_while_others_still_load(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            board.filters.status = crate::model::StatusFilter::Failed;
            board.load_all(cx);
            let failed = RepoResult {
                runs: Err("denied".into()),
                ..result("ann/app", Vec::new())
            };
            board.apply_repo(board.generation, failed, cx);
            assert_eq!(board.error.as_deref(), Some("ann/app: Runs: denied"));
            assert!(matches!(
                board.list.status(),
                subspace_framework::dashboard::list::LoadStatus::Failed { .. }
            ));
            assert!(
                !board.notes.contains_key(&SharedString::from("ann/app")),
                "never read"
            );
            assert_eq!(
                board.notes.get(&SharedString::from("ann/lib")),
                Some(&GroupNote::Loading("Loading runs…".into()))
            );

            board.apply_results(
                board.generation,
                &board.repos.clone(),
                vec![result("ann/lib", Vec::new())],
                cx,
            );
            assert_eq!(
                board.list.groups().len(),
                2,
                "a failed read keeps the groups"
            );
            assert_eq!(
                board.notes.get(&SharedString::from("ann/lib")),
                Some(&GroupNote::Empty(
                    "No failed runs in the latest runs".into()
                ))
            );
        })
        .unwrap();
}

#[gpui_kit::test]
fn the_end_of_a_load_for_another_account_is_ignored(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            board.load_all(cx);
            let stale = board.generation;
            board.switch_account("bob".into(), cx);
            board.apply_results(
                stale,
                &board.repos.clone(),
                vec![result("ann/app", vec![run("ann/app")])],
                cx,
            );
            assert!(
                matches!(board.fetch, Fetch::Loading { .. }),
                "bob's load runs on"
            );
            assert!(!board.data.contains_key("ann"));
            assert!(board.loaded_at.is_none());
        })
        .unwrap();
}
