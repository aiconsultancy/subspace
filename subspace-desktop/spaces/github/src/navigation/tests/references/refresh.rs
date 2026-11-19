//! A reload never lets the lists it is replacing answer for another Space
//!: until the refresh lands they show in the PRs list
//! but answer nothing, and the landing announces `changed`. GPUI's test
//! executor runs the background reads only when the test parks, so every
//! question asked before `run_until_parked` meets a refresh still held.

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;
use subspace_framework::client_test_support::{Mock, Request};
use subspace_framework::contracts::github::ResolutionState;
use subspace_framework::dashboard::Dashboard as _;

use super::{Listener, ask, label, routed, title};
use crate::navigation::tests::loading::client;
use crate::navigation::tests::{KEY, URL, seed};
use crate::{Github, SectionRefresh};

const V1: &str = "Add paging to the work items endpoint";
/// GitHub's GraphQL denial, as for SAML enforcement on the organisation.
const FORBIDDEN: &str =
    r#"{"data":null,"errors":[{"type":"FORBIDDEN","message":"SAML enforcement"}]}"#;
pub(super) const V2: &str = "Add paging to the work items endpoint (v2)";

fn node() -> serde_json::Value {
    json!({
        "__typename":"PullRequest", "id":"fixture-pr", "number":421, "title":V2,
        "url":URL, "state":"OPEN",
        "createdAt":"2026-09-01T00:00:00Z", "updatedAt":"2026-10-02T00:00:00Z",
        "repository":{"nameWithOwner":"octo-org/api"}, "author":{"login":"octo-dev"},
        "bodyHTML":"<p>Refreshed description</p>", "viewerCanReact":true, "changedFiles":2
    })
}

/// GitHub after the pull request changed: V2 in every list and detail.
pub(super) fn github_v2(request: &Request) -> (u16, String) {
    let body = if is_detail_read(request) {
        json!({"data":{"repository":{"issueOrPullRequest":node()}}})
    } else {
        json!({"data":{"search":{"issueCount":1,"nodes":[node()]}}})
    };
    (200, body.to_string())
}

fn is_detail_read(request: &Request) -> bool {
    request.body.contains("issueOrPullRequest")
}

/// The changes heard so far, without running the held reads.
fn count(listener: &Entity<Listener>, cx: &mut VisualTestContext) -> usize {
    cx.update(|_, cx| listener.read(cx).0)
}

/// V1 loaded and answering, then a reload whose refresh is still held.
fn reloaded<'a>(
    cx: &'a mut TestAppContext,
    mock: &Mock,
) -> (Entity<Github>, Entity<Listener>, &'a mut VisualTestContext) {
    let (board, listener, cx) = routed(cx, Some(mock));
    board.update(cx, seed);
    assert_eq!(title(&ask(KEY, cx)).as_deref(), Some(V1));
    board.update_in(cx, |board, window, cx| board.reload(window, cx));
    (board, listener, cx)
}

fn listed_title(board: &Entity<Github>, cx: &mut VisualTestContext) -> Option<String> {
    board.read_with(cx, |board, _| {
        board.find_item(KEY).map(|item| item.title.clone())
    })
}

#[gpui_kit::test]
fn a_held_refresh_never_answers_the_lists_it_replaces(cx: &mut TestAppContext) {
    let mock = Mock::new(github_v2);
    let (board, _, cx) = reloaded(cx, &mock);
    let state = ask(KEY, cx);
    assert_ne!(title(&state).as_deref(), Some(V1), "stale V1 answered");
    assert!(
        matches!(state, ResolutionState::Pending),
        "{}",
        label(&state)
    );
    assert_eq!(
        listed_title(&board, cx).as_deref(),
        Some(V1),
        "the PRs list keeps showing its saved results meanwhile"
    );
}

#[gpui_kit::test]
fn the_refreshed_lists_announce_changed_and_answer_v2(cx: &mut TestAppContext) {
    let mock = Mock::new(github_v2);
    let (board, listener, cx) = reloaded(cx, &mock);
    let held = count(&listener, cx);
    cx.run_until_parked();
    assert!(
        count(&listener, cx) > held,
        "the refreshed lists' landing is announced"
    );
    assert_eq!(title(&ask(KEY, cx)).as_deref(), Some(V2));
    assert_eq!(listed_title(&board, cx).as_deref(), Some(V2));
    cx.run_until_parked();
    let reads = mock.requests().iter().filter(|r| is_detail_read(r)).count();
    assert_eq!(reads, 0, "the refreshed list answers without another read");
}

#[gpui_kit::test]
fn reload_discards_a_section_read_already_in_flight(cx: &mut TestAppContext) {
    let before_reload = Mock::new(github_v2);
    let after_reload = Mock::new(|_| (200, FORBIDDEN.into()));
    let (board, _, cx) = routed(cx, Some(&before_reload));
    board.update(cx, seed);
    board.update_in(cx, |board, window, cx| {
        board.load_all(SectionRefresh::Automatic, cx);
        board.client = Some(client(&after_reload));
        board.reload(window, cx);
    });

    cx.run_until_parked();
    assert!(!before_reload.requests().is_empty());
    assert!(matches!(ask(KEY, cx), ResolutionState::Pending));
    cx.run_until_parked();
    assert!(matches!(ask(KEY, cx), ResolutionState::Unavailable));
    assert_eq!(listed_title(&board, cx).as_deref(), Some(V1));
    assert!(
        after_reload
            .requests()
            .iter()
            .any(|read| !is_detail_read(read)),
        "reload schedules fresh section reads even while old reads are pending"
    );
}

#[gpui_kit::test]
fn the_current_section_read_lands_after_the_search_scope_evicts_its_account(
    cx: &mut TestAppContext,
) {
    let mock = Mock::new(github_v2);
    let (board, _, cx) = routed(cx, Some(&mock));
    board.update(cx, |board, cx| {
        board.load_all(SectionRefresh::Automatic, cx);
        assert!(board.data.remove("octo-dev").is_some());
    });
    cx.run_until_parked();
    assert_eq!(title(&ask(KEY, cx)).as_deref(), Some(V2));
}

/// A refresh GitHub answers with `(status, body)`: the key is read on its
/// own and answers `expected`, never the list the refresh was replacing.
fn refused_refresh(
    answer: (u16, &'static str),
    expected: fn(&ResolutionState) -> bool,
    cx: &mut TestAppContext,
) {
    let mock = Mock::new(move |_| (answer.0, answer.1.into()));
    let (board, _, cx) = reloaded(cx, &mock);
    cx.run_until_parked();
    assert!(matches!(ask(KEY, cx), ResolutionState::Pending));
    cx.run_until_parked();
    let state = ask(KEY, cx);
    assert!(expected(&state), "{}", label(&state));
    assert_eq!(listed_title(&board, cx).as_deref(), Some(V1));
}

#[gpui_kit::test]
fn a_denied_refresh_answers_signed_out_not_the_old_lists(cx: &mut TestAppContext) {
    refused_refresh(
        (401, "Bad credentials"),
        |state| matches!(state, ResolutionState::SignedOut),
        cx,
    );
}

#[gpui_kit::test]
fn a_forbidden_refresh_answers_unavailable_not_the_old_lists(cx: &mut TestAppContext) {
    refused_refresh(
        (200, FORBIDDEN),
        |state| matches!(state, ResolutionState::Unavailable),
        cx,
    );
}

#[gpui_kit::test]
fn a_failed_refresh_answers_failed_not_the_old_lists(cx: &mut TestAppContext) {
    refused_refresh(
        (502, "Bad gateway"),
        |state| matches!(state, ResolutionState::Failed),
        cx,
    );
}

#[gpui_kit::test]
fn a_landed_detail_announces_changed(cx: &mut TestAppContext) {
    let mock = Mock::new(github_v2);
    let (board, listener, cx) = routed(cx, Some(&mock));
    board.update(cx, |board, cx| assert!(board.open_link(URL, cx)));
    let held = count(&listener, cx);
    cx.executor().advance_clock(crate::DETAIL_DEBOUNCE);
    cx.run_until_parked();
    assert!(
        count(&listener, cx) > held,
        "the detail's landing is announced"
    );
    assert_eq!(title(&ask(KEY, cx)).as_deref(), Some(V2));
    assert_eq!(mock.requests().len(), 1, "only the panel's own read");
}
