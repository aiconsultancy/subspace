//! The PRs Space answers `github@1/resolve-pull-requests` with its
//! own current account: from what it already holds, else `Pending` and one
//! read whose landing it announces with `changed`. An account switch, a
//! sign-in or a sign-out forgets every answer and announces `changed`.

use std::sync::Arc;

use gpui_kit::{AppContext as _, Context, Entity, TestAppContext, VisualTestContext};
use serde_json::json;
use subspace_framework::client_test_support::Mock;
use subspace_framework::contracts::github::{
    Changed, CheckStatus, PullRequestStatus, ResolutionState, ResolvePullRequests, ReviewStatus,
};
use subspace_framework::core::Secret;
use subspace_framework::dashboard::Dashboard as _;
use subspace_framework::dashboard::contracts::{self, BoardContracts, Declaration, Role};
use subspace_framework::dashboard::github_sign_in::{self, SignInEvent};
use subspace_framework::github_auth::{AuthError, Authenticator, Started};

use super::loading::{client, response};
use super::{KEY, mount, seed};
use crate::Github;

mod checks;
mod newer;
mod privacy;
mod refresh;

const ASKER: &str = "fixture-channels";
const ASKS: BoardContracts = BoardContracts::new(&[
    Declaration::requests::<ResolvePullRequests>(),
    Declaration::subscribes::<Changed>(),
]);

/// A sign-in the tests never start: they emit its events themselves.
struct Unused;

impl Authenticator for Unused {
    fn start(&self) -> Result<Started, AuthError> {
        Err(AuthError::Cancelled)
    }

    fn revoke(&self, _: &Secret) {}
}

/// Counts the `changed` announcements it hears.
struct Listener(usize);

fn listen(cx: &mut VisualTestContext) -> Entity<Listener> {
    cx.update(|_, cx| {
        let listener = cx.new(|_| Listener(0));
        let subscribed = contracts::subscribe(
            ASKER,
            &listener,
            |listener: &mut Listener, _: &Changed, _: &mut Context<Listener>| listener.0 += 1,
            cx,
        );
        assert!(subscribed.is_ok());
        listener
    })
}

/// The PRs Space with `client`, routed beside a fixture asker.
fn routed<'a>(
    cx: &'a mut TestAppContext,
    mock: Option<&Mock>,
) -> (Entity<Github>, Entity<Listener>, &'a mut VisualTestContext) {
    cx.update(|cx| {
        github_sign_in::install(Arc::new(Unused), cx);
        contracts::install(vec![(ASKER, ASKS), (Github::ID, Github::contracts())], cx);
    });
    let (board, cx) = mount(cx, mock.map(client));
    let listener = listen(cx);
    (board, listener, cx)
}

fn ask(key: &str, cx: &mut VisualTestContext) -> ResolutionState {
    cx.update(|_, cx| {
        let answers = contracts::request(ASKER, &ResolvePullRequests::new([key]), cx)
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(answers.len(), 1);
        answers[0].state.clone()
    })
}

fn heard(listener: &Entity<Listener>, cx: &mut VisualTestContext) -> usize {
    cx.run_until_parked();
    cx.update(|_, cx| listener.read(cx).0)
}

fn title(state: &ResolutionState) -> Option<String> {
    match state {
        ResolutionState::Available(details) => Some(details.title.clone()),
        _ => None,
    }
}

/// What answered, for assertion messages: the contract's states have no
/// `Debug`.
fn label(state: &ResolutionState) -> String {
    match state {
        ResolutionState::Available(details) => format!("Available({})", details.title),
        ResolutionState::Pending => "Pending".into(),
        ResolutionState::SignedOut => "SignedOut".into(),
        ResolutionState::Unavailable => "Unavailable".into(),
        ResolutionState::Failed => "Failed".into(),
        _ => "another state".into(),
    }
}

fn null_repository() -> String {
    json!({"data":{"repository":{"issueOrPullRequest":null}}}).to_string()
}

#[gpui_kit::test]
fn github_declares_that_it_resolves_pull_requests_and_announces_changes(_: &mut TestAppContext) {
    let declared = Github::contracts();
    let names: Vec<(Role, String)> = declared
        .declarations()
        .iter()
        .map(|declaration| (declaration.role(), declaration.key().label()))
        .collect();
    for expected in [
        (Role::Responds, "github@1/open-pull-request"),
        (Role::Responds, "github@1/resolve-pull-requests"),
        (Role::Publishes, "github@1/changed"),
    ] {
        assert!(
            names
                .iter()
                .any(|(role, name)| *role == expected.0 && name == expected.1),
            "{expected:?} in {names:?}"
        );
    }
}

#[gpui_kit::test]
fn loaded_pull_requests_answer_from_memory_by_any_spelling(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (500, String::new()));
    let (board, _, cx) = routed(cx, Some(&mock));
    board.update(cx, seed);
    let state = ask("OCTO-ORG/API#421", cx);
    let ResolutionState::Available(details) = state else {
        panic!("a loaded pull request is available");
    };
    assert_eq!(details.key, KEY);
    assert_eq!(details.title, "Add paging to the work items endpoint");
    assert_eq!(details.url, "https://github.com/octo-org/api/pull/421");
    assert_eq!(details.status, PullRequestStatus::Open);
    assert_eq!(details.author.as_deref(), Some("Ann-Perkins_emu"));
    assert_eq!(details.review, Some(ReviewStatus::ReviewRequired));
    assert_eq!(details.checks, Some(CheckStatus::Pending));
    assert_eq!(details.updated_at.as_deref(), Some("2026-09-30T08:15:00Z"));
    assert!(mock.requests().is_empty(), "no read for what is loaded");
}

/// A completed check run named `name` that ended `conclusion`.
fn run(name: &str, conclusion: &str) -> serde_json::Value {
    json!({"__typename":"CheckRun", "name":name, "status":"COMPLETED",
        "conclusion":conclusion, "detailsUrl":null})
}

/// The read of a pull request whose head commit has five checks, one failed.
fn response_with_checks() -> String {
    let mut body: serde_json::Value = serde_json::from_str(&response("PullRequest", 421)).unwrap();
    let contexts = ["SUCCESS", "SUCCESS", "SKIPPED", "NEUTRAL", "FAILURE"]
        .iter()
        .enumerate()
        .map(|(index, conclusion)| run(&format!("check-{index}"), conclusion))
        .collect::<Vec<_>>();
    body["data"]["repository"]["issueOrPullRequest"]["commits"] = json!({"nodes":[{"commit":
        {"statusCheckRollup":{"state":"FAILURE", "contexts":{"nodes":contexts}}}}]});
    body.to_string()
}

#[gpui_kit::test]
fn a_read_pull_request_counts_the_checks_that_passed(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (200, response_with_checks()));
    let (_, listener, cx) = routed(cx, Some(&mock));
    assert!(matches!(ask(KEY, cx), ResolutionState::Pending));
    assert_eq!(heard(&listener, cx), 1);
    let ResolutionState::Available(details) = ask(KEY, cx) else {
        panic!("the read pull request is available");
    };
    assert_eq!(details.checks, Some(CheckStatus::Failure));
    let counts = details
        .check_counts
        .expect("the read carried the check runs");
    assert_eq!(
        (counts.passed, counts.total),
        (4, 5),
        "skipped and neutral checks pass; the failed one does not"
    );
    assert_eq!(mock.requests().len(), 1, "the same one read, no more");
}

#[gpui_kit::test]
fn an_unknown_pull_request_is_pending_then_read_once_with_the_current_account(
    cx: &mut TestAppContext,
) {
    let mock = Mock::new(|_| (200, response("PullRequest", 421)));
    let (_, listener, cx) = routed(cx, Some(&mock));
    assert!(matches!(ask(KEY, cx), ResolutionState::Pending));
    assert!(matches!(ask(KEY, cx), ResolutionState::Pending));
    assert_eq!(heard(&listener, cx), 1, "the landed read is announced");
    assert_eq!(title(&ask(KEY, cx)).as_deref(), Some("Loaded pull request"));
    assert_eq!(mock.requests().len(), 1, "one read per pull request");
}

#[gpui_kit::test]
fn a_pull_request_the_account_cannot_read_is_unavailable(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (200, null_repository()));
    let (_, listener, cx) = routed(cx, Some(&mock));
    assert!(matches!(ask(KEY, cx), ResolutionState::Pending));
    assert_eq!(heard(&listener, cx), 1);
    assert!(matches!(ask(KEY, cx), ResolutionState::Unavailable));
}

#[gpui_kit::test]
fn an_issue_with_that_number_is_unavailable(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (200, response("Issue", 421)));
    let (_, listener, cx) = routed(cx, Some(&mock));
    ask(KEY, cx);
    assert_eq!(heard(&listener, cx), 1);
    assert!(matches!(ask(KEY, cx), ResolutionState::Unavailable));
}

#[gpui_kit::test]
fn a_server_failure_is_failed(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (502, "Bad gateway".into()));
    let (_, listener, cx) = routed(cx, Some(&mock));
    ask(KEY, cx);
    assert_eq!(heard(&listener, cx), 1);
    assert!(matches!(ask(KEY, cx), ResolutionState::Failed));
}

#[gpui_kit::test]
fn without_github_or_an_account_it_answers_signed_out(cx: &mut TestAppContext) {
    let (board, _, cx) = routed(cx, None);
    assert!(matches!(ask(KEY, cx), ResolutionState::SignedOut));
    let mock = Mock::new(|_| (200, response("PullRequest", 421)));
    board.update(cx, |board, _| {
        board.client = Some(client(&mock));
        board.account = None;
    });
    assert!(matches!(ask(KEY, cx), ResolutionState::SignedOut));
    assert!(mock.requests().is_empty());
}

#[gpui_kit::test]
fn an_account_switch_forgets_answers_and_announces_changed(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (200, null_repository()));
    let (board, listener, cx) = routed(cx, Some(&mock));
    board.update(cx, seed);
    assert!(title(&ask(KEY, cx)).is_some());
    let before = heard(&listener, cx);
    board.update(cx, |board, cx| {
        board.set_account("other-account".into(), cx)
    });
    assert!(heard(&listener, cx) > before, "the switch is announced");
    let state = ask(KEY, cx);
    assert!(
        title(&state).is_none(),
        "the other account's details never answer for this one"
    );
}

fn emit(event: SignInEvent, cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        let sign_in = github_sign_in::shared(cx).expect("an installed sign-in");
        sign_in.update(cx, |_, cx| cx.emit(event));
    });
}

#[gpui_kit::test]
fn signing_out_answers_signed_out_until_someone_signs_in(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (200, null_repository()));
    let (board, listener, cx) = routed(cx, Some(&mock));
    board.update(cx, seed);
    assert!(title(&ask(KEY, cx)).is_some());
    let before = heard(&listener, cx);
    emit(SignInEvent::SignedOut("OCTO-DEV".into()), cx);
    assert!(heard(&listener, cx) > before, "the sign-out is announced");
    assert!(
        matches!(ask(KEY, cx), ResolutionState::SignedOut),
        "nothing the signed-out account read answers"
    );
    let before = heard(&listener, cx);
    emit(SignInEvent::SignedIn("octo-dev".into()), cx);
    assert!(heard(&listener, cx) > before, "the sign-in is announced");
    assert!(title(&ask(KEY, cx)).is_some());
    assert!(mock.requests().is_empty());
}

#[gpui_kit::test]
fn another_account_signing_out_forgets_reads_and_reads_again(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (200, null_repository()));
    let (_, listener, cx) = routed(cx, Some(&mock));
    ask(KEY, cx);
    heard(&listener, cx);
    assert!(matches!(ask(KEY, cx), ResolutionState::Unavailable));
    emit(SignInEvent::SignedOut("other-account".into()), cx);
    heard(&listener, cx);
    assert!(matches!(ask(KEY, cx), ResolutionState::Pending));
    heard(&listener, cx);
    assert_eq!(mock.requests().len(), 2, "read again after the sign-out");
}

#[gpui_kit::test]
fn reloading_forgets_answers_and_announces_changed(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (200, response("PullRequest", 421)));
    let (board, listener, cx) = routed(cx, Some(&mock));
    ask(KEY, cx);
    assert_eq!(heard(&listener, cx), 1);
    board.update_in(cx, |board, window, cx| board.reload(window, cx));
    assert!(heard(&listener, cx) >= 2);
}
