//! Pull request references end to end through the real broker: a
//! fixture Space answers the `github` contract in place of PRs, and the
//! fallback when no Space answers or PRs is not running.

mod hover;
mod interaction;
mod keyboard;

use std::collections::HashMap;

use gpui_kit::base::test_support::snapshots;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{App, AppContext as _, Entity, TestAppContext, VisualTestContext};
use subspace_framework::contracts::github::{
    Changed, CheckStatus, OpenPullRequest, PullRequestDetails, PullRequestStatus, Resolution,
    ResolutionState, ResolvePullRequests, ReviewStatus,
};
use subspace_framework::dashboard::Dashboard as _;
use subspace_framework::dashboard::contracts::{self, BoardContracts, Declaration};

use super::{open_with_message, settings};
use crate::hub::{self, Hub};
use crate::{Irc, pull_requests};

const PRS: &str = "fixture-prs";
const PRS_CONTRACTS: BoardContracts = BoardContracts::new(&[
    Declaration::responds::<ResolvePullRequests>(),
    Declaration::responds::<OpenPullRequest>(),
    Declaration::publishes::<Changed>(),
]);
pub(super) const KEY: &str = "octo-org/api#421";
pub(super) const URL: &str = "https://github.com/Octo-Org/API/pull/421/files?w=1#r9";

/// Answers like the PRs Space: from memory, `Pending` for what it has not
/// read yet. Records every request it is asked.
pub(super) struct Board {
    pub answers: HashMap<String, ResolutionState>,
    pub asked: Vec<Vec<String>>,
}

impl Board {
    fn resolve(&mut self, request: &ResolvePullRequests) -> Vec<Resolution> {
        self.asked.push(request.keys().to_vec());
        request
            .keys()
            .iter()
            .map(|key| {
                let state = self.answers.get(key).cloned();
                Resolution::new(key, state.unwrap_or(ResolutionState::Pending))
            })
            .collect()
    }
}

pub(super) fn details() -> PullRequestDetails {
    PullRequestDetails::new(
        KEY,
        "Keep pull request context beside the conversation",
        "https://github.com/octo-org/api/pull/421",
        PullRequestStatus::Open,
    )
    .with_author(Some("octo-dev"))
    .with_review(Some(ReviewStatus::Approved))
    .with_checks(Some(CheckStatus::Success))
    .with_updated_at(Some("2026-11-16T11:56:00Z"))
    .with_branches(Some("feat/chips"), Some("main"))
    .with_changes(Some(120), Some(8))
}

/// How the fixture PRs Space takes part: not at all, routed but not
/// running (no responder attached), or running.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Prs {
    Absent,
    Running,
}

fn install(prs: Prs, cx: &mut App) -> (Entity<Hub>, Option<Entity<Board>>) {
    let mut boards = vec![(crate::ID, Irc::contracts())];
    if prs == Prs::Running {
        boards.push((PRS, PRS_CONTRACTS));
    }
    contracts::install(boards, cx);
    let board = (prs == Prs::Running).then(|| attach_board(cx));
    let hub = hub::install_for_test(&settings(), cx);
    pull_requests::attach(&hub, cx);
    (hub, board)
}

fn attach_board(cx: &mut App) -> Entity<Board> {
    let board = cx.new(|_| Board {
        answers: HashMap::new(),
        asked: Vec::new(),
    });
    let resolves = contracts::respond(
        PRS,
        &board,
        |board: &mut Board, request: &ResolvePullRequests, _| board.resolve(request),
        cx,
    );
    assert!(resolves.is_ok());
    board
}

/// The board showing `#subspace` with bob's message `text` last.
pub(super) fn open_board<'a>(
    cx: &'a mut TestAppContext,
    prs: Prs,
    text: &str,
) -> (
    Entity<Irc>,
    Entity<Hub>,
    Option<Entity<Board>>,
    &'a mut VisualTestContext,
) {
    open_with_message(cx, |cx| install(prs, cx), text)
}

/// The inline reference triggers drawn, in reading order.
pub(super) fn chips(cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        let mut found: Vec<(gpui_kit::Pixels, String)> = snapshots(window)
            .into_iter()
            .filter_map(|node| {
                let id = node.path().last()?.to_string();
                id.starts_with("irc-reference:")
                    .then(|| (node.bounds().origin.x, id))
            })
            .collect();
        found.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        found.into_iter().map(|(_, id)| id).collect()
    })
}

pub(super) fn answer(board: &Entity<Board>, state: ResolutionState, cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        board.update(cx, |board, _| {
            board.answers.insert(KEY.into(), state);
        });
        assert!(contracts::publish(PRS, Changed, cx).is_ok());
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_marker_and_a_url_for_one_pull_request_ask_once_by_its_identity(cx: &mut TestAppContext) {
    let text = format!("[PR:Octo-Org/API#421] and {URL}, also [PR:octo-org/web#7]");
    let (_, _, board, cx) = open_board(cx, Prs::Running, &text);
    let board = board.expect("a PRs fixture");
    cx.update(|_, cx| {
        assert_eq!(
            board.read(cx).asked,
            [vec![KEY.to_owned(), "octo-org/web#7".to_owned()]],
            "one request, one key per pull request"
        );
    });
    assert_eq!(chips(cx).len(), 3, "the marker and the URL are both chips");
}

#[gpui_kit::test]
fn changed_asks_again_for_the_pull_requests_shown(cx: &mut TestAppContext) {
    let (_, _, board, cx) = open_board(cx, Prs::Running, "[PR:octo-org/api#421]");
    let board = board.expect("a PRs fixture");
    answer(&board, ResolutionState::SignedOut, cx);
    cx.update(|_, cx| {
        assert_eq!(board.read(cx).asked.len(), 2, "asked again after changed");
    });
}

#[gpui_kit::test]
fn bare_numbers_and_repo_only_markers_stay_plain_text(cx: &mut TestAppContext) {
    let (_, _, board, cx) = open_board(cx, Prs::Running, "see #123 and [PR:api#9]");
    let board = board.expect("a PRs fixture");
    cx.update(|_, cx| assert!(board.read(cx).asked.is_empty()));
    assert!(
        chips(cx).iter().all(|id| !id.contains("pr")),
        "{:?}",
        chips(cx)
    );
}

#[gpui_kit::test]
fn chips_name_the_owner_only_to_tell_same_named_repositories_apart(cx: &mut TestAppContext) {
    let text = "[PR:octo-org/api#421] [PR:acme/api#5] [PR:octo-org/web#7]";
    let (_, _, _, cx) = open_board(cx, Prs::Running, text);
    let shown = cx.update(|window, cx| {
        window.render_frame(cx);
        gpui_kit::base::test_support::texts(window)
            .into_iter()
            .filter(|text| {
                text.path()
                    .iter()
                    .any(|id| id.to_string().contains("irc-reference:"))
            })
            .map(|text| text.text().to_owned())
            .collect::<Vec<_>>()
    });
    for label in ["octo-org/api#421", "acme/api#5", "web#7"] {
        assert!(
            shown.iter().any(|text| text == label),
            "{label} in {shown:?}"
        );
    }
    assert!(
        !shown.iter().any(|text| text == "octo-org/web#7"),
        "{shown:?}"
    );
}
