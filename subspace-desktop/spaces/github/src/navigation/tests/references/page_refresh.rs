//! Toolbar Refresh uses the open PR and retains its content during failures.

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use serde_json::json;
use subspace_framework::client_github::model::PrState;
use subspace_framework::client_test_support::Mock;

use super::refresh::{is_detail_read, node};
use super::routed;
use crate::navigation::tests::{KEY, seed};

#[gpui_kit::test]
fn the_pr_page_refresh_keeps_the_open_pr_when_it_merges(cx: &mut TestAppContext) {
    let mock = Mock::new(|request| {
        let body = if is_detail_read(request) {
            let mut merged = node();
            merged["state"] = json!("MERGED");
            json!({"data":{"repository":{"issueOrPullRequest":merged}}})
        } else {
            json!({"data":{"search":{"issueCount":0,"nodes":[]}}})
        };
        (200, body.to_string())
    });
    let (board, _, cx) = routed(cx, Some(&mock));
    board.update(cx, |board, cx| {
        seed(board, cx);
        crate::page::open_page(board, KEY, cx);
        // Another row's debounced read is held while this cached PR is open.
        board.fetch_detail("octo-org/api#422".into(), true, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, cx| window.click("gh-page-refresh", cx));
    cx.run_until_parked();
    assert!(
        mock.requests()
            .iter()
            .any(|request| { is_detail_read(request) && request.body.contains(r#""number":421"#) })
    );
    board.read_with(cx, |board, _| {
        assert_eq!(board.list.open_row().map(|row| row.key.as_ref()), Some(KEY));
        assert!(matches!(
            board.find_item(KEY).map(|item| item.state),
            Some(PrState::Merged)
        ));
    });
}

#[gpui_kit::test]
fn the_pr_page_refresh_shows_the_failure_beside_cached_content(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (503, "Service unavailable".into()));
    let (board, _, cx) = routed(cx, Some(&mock));
    board.update(cx, |board, cx| {
        seed(board, cx);
        crate::page::open_page(board, KEY, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, cx| window.click("gh-page-refresh", cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, _| {
        let error = window.find("github-detail-error-octo-org/api#421").bounds();
        assert!(error.size.height > gpui_kit::px(0.0));
    });
    board.read_with(cx, |board, _| {
        assert_eq!(
            board.data["octo-dev"].details[KEY].body_html,
            "<p>Cached pull request</p>"
        );
    });
}
