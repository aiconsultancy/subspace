use std::{cell::RefCell, rc::Rc};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, Subscription, TestAppContext, VisualTestContext};
use serde_json::json;
use subspace_framework::contracts::github::OpenPullRequest;
use subspace_framework::dashboard::contracts::{
    self, BoardContracts, Declaration, MessageKey, Role,
};
use subspace_framework::dashboard::{BoardNavigation, CommandRun, Dashboard as _, DashboardEvent};

use crate::Issues;
use crate::tests::offline::{Offline, offline_with, settle_board};
use crate::tests::{has, label, press, render};

const URL: &str = "https://github.com/octo-org/api/pull/421";
const RESPONDS: BoardContracts = BoardContracts::new(&[Declaration::responds::<OpenPullRequest>()]);

#[derive(Default)]
struct Events {
    navigations: Vec<BoardNavigation>,
    notices: Vec<String>,
}

fn observe(
    board: &Entity<Issues>,
    cx: &mut VisualTestContext,
) -> (Rc<RefCell<Events>>, Subscription) {
    let events = Rc::new(RefCell::new(Events::default()));
    let received = events.clone();
    let subscription = cx.update(|_, cx| {
        cx.subscribe(board, move |_, event, _| match event {
            DashboardEvent::Navigate(navigation) => {
                received.borrow_mut().navigations.push(navigation.clone())
            }
            DashboardEvent::Toast(toast) => {
                received.borrow_mut().notices.push(toast.text.to_string())
            }
            _ => {}
        })
    });
    (events, subscription)
}

fn fixture(url: &str) -> Offline {
    let url = url.to_owned();
    offline_with(move |request| {
        if request.path.ends_with("/issues/SUB-1") {
            let mut detail =
                crate::tests::offline::api("/api/v1/issues/SUB-1", &Default::default());
            detail["pullRequestSummary"] = json!({"total": 7, "open": 2, "merged": 5,
                "closed": 0, "closingPending": 0, "lastObservedAt": null});
            return Some((200, detail.to_string()));
        }
        request.path.contains("/pull-requests?").then(|| {
        (200, json!({"items":[{
            "id":"fixture-pr", "repository":{"owner":"octo-org","name":"api","fullName":"octo-org/api","url":"https://github.com/octo-org/api"},
            "number":421,"url":url,"title":"Fixture pull request","state":"open","isDraft":false,
            "headBranch":"fixture","baseBranch":"main","referenceKind":"closes","referenceSources":[],
            "referenceActive":true,"completion":"pending","githubUpdatedAt":"2026-11-01T00:00:00Z","lastObservedAt":"2026-11-01T00:00:00Z"
        }],"nextCursor":null}).to_string())
    })
    })
}

fn ready(board: &Entity<Issues>, cx: &mut VisualTestContext, responds: bool) {
    settle_board(board, cx, |board| {
        board
            .issue_view
            .snapshot
            .as_ref()
            .is_some_and(|s| s.pull_requests().is_some())
    });
    cx.update(|_, cx| {
        let mut boards = vec![(crate::ID, Issues::contracts())];
        if responds {
            boards.push(("fixture-prs", RESPONDS));
        }
        contracts::install(boards, cx);
    });
    render(cx);
}

#[gpui_kit::test]
fn row_click_enter_and_palette_emit_the_declared_navigation(cx: &mut TestAppContext) {
    let fixture = fixture(URL);
    let (board, cx) = super::opened(&fixture, cx);
    ready(&board, cx, true);
    let (events, _subscription) = observe(&board, cx);
    assert_eq!(
        label(cx, "issue-pull-requests").as_deref(),
        Some("Pull requests · 2 open · 5 merged")
    );
    assert_eq!(
        label(cx, "issue-pr-status:fixture-pr").as_deref(),
        Some("Open")
    );
    assert_eq!(
        label(cx, "issue-pr:fixture-pr").as_deref(),
        Some("Open octo-org/api#421 in PRs: Fixture pull request, Open")
    );
    cx.update(|window, cx| window.click("issue-pr:fixture-pr", cx));
    for _ in 0..30 {
        press(cx, "tab");
        if cx.update(|window, _| window.find("issue-pr:fixture-pr").focused()) == Some(true) {
            break;
        }
    }
    cx.update(|window, _| assert_eq!(window.find("issue-pr:fixture-pr").focused(), Some(true)));
    press(cx, "enter");
    cx.update(|window, cx| {
        let commands = board.read(cx).palette_commands();
        let command = commands
            .iter()
            .find(|c| c.label == "Open in PRs: octo-org/api#421")
            .unwrap();
        let CommandRun::Callback(run) = &command.run else {
            panic!("expected callback")
        };
        run(window, cx);
    });
    cx.run_until_parked();
    let events = events.borrow();
    assert_eq!(events.navigations.len(), 3);
    for navigation in &events.navigations {
        assert_eq!(navigation.url.as_ref(), URL);
        assert_eq!(
            navigation.responder,
            MessageKey::request::<OpenPullRequest>()
        );
    }
    assert!(events.notices.is_empty());
    assert!(cx.opened_url().is_none());
    assert!(has(cx, "issues-detail"));
}

#[gpui_kit::test]
fn unavailable_prs_falls_back_with_a_notice(cx: &mut TestAppContext) {
    let fixture = fixture(URL);
    let (board, cx) = super::opened(&fixture, cx);
    ready(&board, cx, false);
    let (events, _subscription) = observe(&board, cx);
    cx.update(|window, cx| window.click("issue-pr:fixture-pr", cx));
    cx.run_until_parked();
    assert_eq!(cx.opened_url().as_deref(), Some(URL));
    assert_eq!(
        events.borrow().notices,
        ["PRs is unavailable. Opening in browser."]
    );
    assert!(events.borrow().navigations.is_empty());
    assert!(has(cx, "issues-detail"));
}

#[gpui_kit::test]
fn secondary_click_enter_and_palette_open_the_browser(cx: &mut TestAppContext) {
    let fixture = fixture(URL);
    let (board, cx) = super::opened(&fixture, cx);
    ready(&board, cx, true);
    let (events, _subscription) = observe(&board, cx);
    cx.update(|window, cx| window.click("issue-pr-browser:fixture-pr", cx));
    assert_eq!(cx.opened_url().as_deref(), Some(URL));
    cx.update(|_, cx| cx.open_url("https://example.test/before-enter"));
    // Clicking a Link focuses it; Enter retains the panel's native Link path.
    press(cx, "enter");
    assert_eq!(cx.opened_url().as_deref(), Some(URL));
    cx.update(|window, cx| {
        cx.open_url("https://example.test/before-palette");
        let commands = board.read(cx).palette_commands();
        let command = commands
            .iter()
            .find(|c| c.label == "Open on GitHub: octo-org/api#421")
            .unwrap();
        let CommandRun::Callback(run) = &command.run else {
            panic!("expected callback")
        };
        run(window, cx);
    });
    cx.run_until_parked();
    assert_eq!(cx.opened_url().as_deref(), Some(URL));
    assert!(events.borrow().notices.is_empty());
    assert!(events.borrow().navigations.is_empty());
}

#[gpui_kit::test]
fn invalid_pr_link_falls_back_without_navigation(cx: &mut TestAppContext) {
    let url = "https://github.com/octo-org/api/issues/421";
    let fixture = fixture(url);
    let (board, cx) = super::opened(&fixture, cx);
    ready(&board, cx, true);
    let (events, _subscription) = observe(&board, cx);
    cx.update(|window, cx| window.click("issue-pr:fixture-pr", cx));
    cx.run_until_parked();
    assert_eq!(cx.opened_url().as_deref(), Some(url));
    assert_eq!(
        events.borrow().notices,
        ["This link cannot open in PRs. Opening in browser."]
    );
    assert!(events.borrow().navigations.is_empty());
    assert!(has(cx, "issues-detail"));
}

#[gpui_kit::test]
fn unsafe_link_is_blocked_and_reports_the_failure(cx: &mut TestAppContext) {
    let fixture = fixture("javascript:alert(1)");
    let (board, cx) = super::opened(&fixture, cx);
    ready(&board, cx, true);
    let (events, _subscription) = observe(&board, cx);
    cx.update(|window, cx| window.click("issue-pr:fixture-pr", cx));
    cx.run_until_parked();
    assert!(cx.opened_url().is_none());
    assert_eq!(events.borrow().notices, ["This link cannot be opened."]);
    assert!(events.borrow().navigations.is_empty());
}

#[test]
fn issues_declares_navigation_and_retains_its_existing_roles() {
    use subspace_framework::contracts::issues::{Changed, CurrentScope, Refresh, Resolve};
    let declared = Issues::contracts();
    assert_eq!(declared.declarations().len(), 5);
    for (role, key) in [
        (Role::Requests, MessageKey::request::<OpenPullRequest>()),
        (Role::Responds, MessageKey::request::<CurrentScope>()),
        (Role::Responds, MessageKey::request::<Resolve>()),
        (Role::Responds, MessageKey::request::<Refresh>()),
        (Role::Publishes, MessageKey::event::<Changed>()),
    ] {
        assert!(declared.declares(role, key));
    }
    assert!(contracts::validate(&[(crate::ID, declared), ("fixture-prs", RESPONDS)]).is_ok());
}
