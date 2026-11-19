//! Request scheduling through the board lifecycle, without running network tasks.
use crate::{Account, Github, SectionStatus};
use gpui_kit::{TestAppContext, Window};
use std::sync::Arc;
use std::time::{Duration, Instant};
use subspace_framework::client_github::Client;
use subspace_framework::dashboard::Dashboard as _;

fn board(window: &mut Window, cx: &mut gpui_kit::Context<Github>) -> Github {
    let mut board = Github::new(window, cx);
    let config = subspace_framework::core::config::Config::default();
    board.client = Some(Arc::new(
        Client::new(subspace_framework::agent_tools::test_support::credentials(
            &config,
        ))
        .with_api_url("http://127.0.0.1:0"),
    ));
    board.started = true;
    board.account = Some("ann".into());
    board.accounts = vec!["ann".into()];
    board.data.insert("ann".into(), Account::default());
    board
}

#[gpui_kit::test]
fn automatic_refresh_reuses_a_recent_list_but_manual_refresh_is_immediate(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let window = cx.add_window(board);
    window
        .update(cx, |board, window, cx| {
            let recent = Instant::now() - Duration::from_secs(60);
            board.data.get_mut("ann").unwrap().loaded_at = Some(recent);
            board.background_refresh(cx);
            assert_eq!(board.data["ann"].loaded_at, Some(recent));
            assert!(board.data["ann"].status.is_empty());
            board.reload(window, cx);
            assert_eq!(board.data["ann"].status.len(), 4);
            assert!(
                board.data["ann"]
                    .status
                    .values()
                    .all(|status| *status == SectionStatus::Loading)
            );
            assert!(board.data["ann"].loaded_at.unwrap() > recent);
        })
        .unwrap();
}

#[gpui_kit::test]
fn automatic_refresh_uses_the_ten_minute_actions_cadence(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            let recent = Instant::now() - Duration::from_secs(540);
            board.data.get_mut("ann").unwrap().loaded_at = Some(recent);
            board.background_refresh(cx);
            assert_eq!(board.data["ann"].loaded_at, Some(recent));
            assert!(board.data["ann"].status.is_empty());

            let expired = Instant::now() - Duration::from_secs(600);
            board.data.get_mut("ann").unwrap().loaded_at = Some(expired);
            board.background_refresh(cx);
            assert!(board.data["ann"].loaded_at.unwrap() > expired);
            assert_eq!(board.data["ann"].status.len(), 4);
        })
        .unwrap();
}

#[gpui_kit::test]
fn repeated_manual_refresh_does_not_start_another_list_request(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let window = cx.add_window(board);
    window
        .update(cx, |board, window, cx| {
            board.reload(window, cx);
            let started = board.data["ann"].loaded_at.unwrap();
            board.data.get_mut("ann").unwrap().loaded_at = Some(started - Duration::from_secs(1));
            board.reload(window, cx);
            assert_eq!(
                board.data["ann"].loaded_at,
                Some(started - Duration::from_secs(1))
            );
        })
        .unwrap();
}

#[gpui_kit::test]
fn disabled_automatic_refresh_still_allows_manual_reload(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        subspace_framework::dashboard::prefs::init_file(None, cx);
        subspace_framework::dashboard::refresh::Preferences {
            enabled: false,
            interval_seconds: None,
        }
        .save("github", cx);
    });
    let window = cx.add_window(board);
    window
        .update(cx, |board, window, cx| {
            let expired = Instant::now() - Duration::from_secs(600);
            board.data.get_mut("ann").unwrap().loaded_at = Some(expired);
            board.background_refresh(cx);
            assert!(board.data["ann"].status.is_empty());
            assert_eq!(board.data["ann"].loaded_at, Some(expired));
            board.reload(window, cx);
            assert_eq!(board.data["ann"].status.len(), 4);
        })
        .unwrap();
}

#[gpui_kit::test]
fn saved_interval_changes_replace_the_default_without_restart(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        subspace_framework::dashboard::prefs::init_file(None, cx);
    });
    let window = cx.add_window(board);
    window
        .update(cx, |board, _, cx| {
            let loaded = Instant::now() - Duration::from_secs(400);
            board.data.get_mut("ann").unwrap().loaded_at = Some(loaded);
            subspace_framework::dashboard::refresh::Preferences {
                enabled: true,
                interval_seconds: Some(600),
            }
            .save("github", cx);
            board.background_refresh(cx);
            assert!(board.data["ann"].status.is_empty());
            subspace_framework::dashboard::refresh::Preferences {
                enabled: true,
                interval_seconds: Some(30),
            }
            .save("github", cx);
            board.background_refresh(cx);
            assert_eq!(board.data["ann"].status.len(), 4);
        })
        .unwrap();
}
