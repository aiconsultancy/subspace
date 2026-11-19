//! Detail reads through the current account, including targets absent from lists.
use std::sync::Arc;

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;
use subspace_framework::client_github::Client;
use subspace_framework::client_test_support::Mock;
use subspace_framework::dashboard::Dashboard as _;
use subspace_framework::dashboard::list::{Body, ListDashboard as _};

use super::{KEY, URL, mount, render, seed};
use crate::{Account, Github};

pub(super) fn response(kind: &str, number: u32) -> String {
    json!({"data":{"repository":{"issueOrPullRequest":{
        "__typename":kind, "id":"fixture-pr", "number":number, "title":"Loaded pull request",
        "url":format!("https://github.com/octo-org/api/pull/{number}"), "state":"OPEN",
        "createdAt":"2026-09-01T00:00:00Z", "updatedAt":"2026-09-02T00:00:00Z",
        "repository":{"nameWithOwner":"octo-org/api"}, "author":{"login":"octo-dev"},
        "bodyHTML":"<p>Loaded description</p>", "viewerCanReact":true, "changedFiles":2
    }}}})
    .to_string()
}

pub(super) fn client(mock: &Mock) -> Arc<Client> {
    let mut config = subspace_framework::core::config::Config::default();
    config.github.token = Some(subspace_framework::core::Secret::new(
        "fixture-current-account",
    ));
    Arc::new(
        Client::new(subspace_framework::agent_tools::test_support::credentials(
            &config,
        ))
        .with_api_url(mock.base_url()),
    )
}

fn finish(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(crate::DETAIL_DEBOUNCE);
    cx.run_until_parked();
    render(cx);
}

fn open_fixture<'a>(
    cx: &'a mut TestAppContext,
    mock: &Mock,
    url: &str,
) -> (Entity<Github>, &'a mut VisualTestContext) {
    let (board, cx) = mount(cx, Some(client(mock)));
    board.update(cx, |board, cx| assert!(board.open_link(url, cx)));
    finish(cx);
    (board, cx)
}

#[gpui_kit::test]
fn unknown_pr_loads_with_the_current_account_without_changing_section_results(
    cx: &mut TestAppContext,
) {
    let mock = Mock::new(|_| (200, response("PullRequest", 421)));
    let (board, cx) = mount(cx, Some(client(&mock)));
    board.update(cx, |board, cx| {
        seed(board, cx);
        let mut other = board.data.remove("octo-dev").unwrap();
        other.details.get_mut(KEY).unwrap().item.title = "Other account's pull request".into();
        board.data.insert("other-account".into(), other);
        board.data.insert("octo-dev".into(), Account::default());
        board.rebuild(cx);
        assert!(board.find_item(KEY).is_none());
        assert!(board.open_link(URL, cx));
        assert_eq!(board.list.open_row().unwrap().key.as_ref(), KEY);
        assert_eq!(
            board.detail(KEY).unwrap().sections[0].body,
            Body::Text("Loading…".into())
        );
        // A list refresh must not discard the pending foreground detail read.
        crate::search_index::start_prefetch(board, cx);
    });
    finish(cx);
    board.read_with(cx, |board, _| {
        assert_eq!(
            board.detail(KEY).unwrap().title.as_ref(),
            "Loaded pull request"
        );
        assert_eq!(
            board.list.open_row().unwrap().title.as_ref(),
            "Loaded pull request"
        );
        assert_eq!(board.account.as_deref(), Some("octo-dev"));
        assert!(board.data["octo-dev"].sections.is_empty());
        assert_eq!(
            board.data["other-account"].details[KEY].item.title,
            "Other account's pull request"
        );
    });
    assert_current_request(&mock);
}

fn assert_current_request(mock: &Mock) {
    let requests = mock.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/graphql");
    let body: serde_json::Value = serde_json::from_str(&requests[0].body).unwrap();
    assert_eq!(
        body["variables"],
        json!({"owner":"octo-org", "name":"api", "number":421})
    );
    assert!(
        requests[0]
            .headers
            .iter()
            .any(|(k, v)| k == "authorization" && v == "Bearer fixture-current-account")
    );
}

#[gpui_kit::test]
fn failed_or_non_pr_reads_leave_an_error_in_the_requested_panel(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (200, response("Issue", 421)));
    let (board, cx) = open_fixture(cx, &mock, URL);
    board.read_with(cx, |board, _| {
        assert!(board.find_item(KEY).is_none());
        assert_eq!(board.list.open_row().unwrap().key.as_ref(), KEY);
        assert!(matches!(&board.detail(KEY).unwrap().sections[0].body, Body::Text(text) if text.contains("Pull request not found")));
    });
}

#[gpui_kit::test]
fn unknown_case_variant_uses_githubs_identity_for_the_panel_and_actions(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (200, response("PullRequest", 421)));
    let (board, cx) = open_fixture(cx, &mock, "https://github.com/OCTO-ORG/API/pull/421");
    board.read_with(cx, |board, _| {
        assert_eq!(board.list.open_row().unwrap().key.as_ref(), KEY);
        let item = board.selected_item().unwrap();
        assert_eq!(item.key, KEY);
        assert!(board.data["octo-dev"].details.contains_key(&item.key));
        assert_eq!(board.list.rows().count(), 1);
    });
    assert_eq!(mock.requests().len(), 1);
}

#[gpui_kit::test]
fn missing_pr_keeps_the_error_visible_and_can_be_retried(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| {
        (
            200,
            json!({"data":{"repository":{"issueOrPullRequest":null}}}).to_string(),
        )
    });
    let (board, cx) = open_fixture(cx, &mock, URL);
    board.read_with(cx, |board, _| {
        assert!(board.find_item(KEY).is_none());
        assert!(matches!(&board.detail(KEY).unwrap().sections[0].body, Body::Text(text) if text.contains("Not found")));
    });
    board.update(cx, |board, cx| assert!(board.open_link(URL, cx)));
    finish(cx);
    assert_eq!(mock.requests().len(), 2);
}

#[gpui_kit::test]
fn closing_a_loaded_out_of_list_pr_discards_its_row(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (200, response("PullRequest", 421)));
    let (board, cx) = open_fixture(cx, &mock, URL);
    board.read_with(cx, |board, _| assert_eq!(board.list.rows().count(), 1));
    board.update_in(cx, |board, window, cx| board.focus.focus(window, cx));
    cx.simulate_keystrokes("escape");
    board.update(cx, |board, cx| {
        assert!(board.navigation.is_none());
        assert_eq!(board.list.rows().count(), 0);
        board.section_action(1, cx);
        board.rebuild(cx);
        assert_eq!(board.list.rows().count(), 0);
        assert!(board.list.open_row().is_none());
    });
}
