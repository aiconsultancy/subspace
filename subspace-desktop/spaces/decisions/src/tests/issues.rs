//! Linked issues open in the Space that answers the `issues` contract,
//! through the shell; this Space never names it.

use gpui_kit::{AppContext as _, TestAppContext};
use subspace_framework::contracts::issues::{CurrentScope, IssueScope};
use subspace_framework::dashboard::Dashboard as _;
use subspace_framework::dashboard::contracts::{self, BoardContracts, Declaration, MessageKey};

use crate::Decisions;
use crate::tests::{click, click_row, mount, observe};

/// A stand-in for the Space that answers for issues.
struct Answering;

const ANSWERS: BoardContracts = BoardContracts::new(&[Declaration::responds::<CurrentScope>()]);

fn scope() -> IssueScope {
    IssueScope::new("subspace-api", "Subspace API", "http://127.0.0.1:3000/api")
        .with_project_keys(["NAV", "WARP"])
}

#[gpui_kit::test]
fn a_linked_issue_opens_in_the_space_that_answers_for_issues(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    let _answering = cx.update(|_, cx| {
        let boards = vec![
            (crate::ID, Decisions::contracts()),
            ("fixture-issues", ANSWERS),
        ];
        contracts::install(boards, cx);
        let answering = cx.new(|_| Answering);
        contracts::respond(
            "fixture-issues",
            &answering,
            |_, _: &CurrentScope, _| Some(scope()),
            cx,
        )
        .unwrap();
        answering
    });
    let (events, _subscription) = observe(&board, cx);
    click_row(cx, "d2");
    click(cx, "decisions-issue:NAV-2");
    let events = events.borrow();
    assert_eq!(events.navigations.len(), 1);
    let navigation = &events.navigations[0];
    assert_eq!(
        navigation.url.as_ref(),
        "http://127.0.0.1:3000/api/v1/issues/NAV-2?profile=subspace-api"
    );
    assert_eq!(navigation.responder, MessageKey::request::<CurrentScope>());
    assert!(events.notices.is_empty());
}

#[gpui_kit::test]
fn without_a_space_answering_for_issues_the_issue_says_why_it_did_not_open(
    cx: &mut TestAppContext,
) {
    let (board, cx) = mount(cx);
    let (events, _subscription) = observe(&board, cx);
    click_row(cx, "d2");
    click(cx, "decisions-issue:NAV-5");
    let events = events.borrow();
    assert!(events.navigations.is_empty());
    assert_eq!(events.notices.len(), 1);
    assert!(events.notices[0].starts_with("NAV-5 can't open here."));
}
