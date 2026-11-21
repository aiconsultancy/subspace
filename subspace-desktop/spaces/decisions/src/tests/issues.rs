//! Linked issues open in the Space that answers the `issues` contract,
//! through the shell; this Space never names it.

use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use subspace_framework::contracts::issues::{CurrentScope, IssueScope};
use subspace_framework::dashboard::Dashboard as _;
use subspace_framework::dashboard::contracts::{self, BoardContracts, Declaration, MessageKey};

use crate::Decisions;
use crate::tests::{click, click_row, has, mount, observe, selected};

/// A stand-in for the Space that answers for issues.
struct Answering;

const ANSWERS: BoardContracts = BoardContracts::new(&[Declaration::responds::<CurrentScope>()]);

fn scope() -> IssueScope {
    IssueScope::new("subspace-api", "Subspace API", "http://127.0.0.1:3000/api")
        .with_project_keys(["NAV", "WARP"])
}

/// Installs the stand-in as the Space that answers for issues.
fn answer_for_issues(cx: &mut VisualTestContext) -> Entity<Answering> {
    cx.update(|_, cx| {
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
    })
}

#[gpui_kit::test]
fn a_linked_issue_opens_in_the_space_that_answers_for_issues(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    let _answering = answer_for_issues(cx);
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

#[gpui_kit::test]
fn back_from_an_issue_chosen_in_the_list_returns_to_its_decision(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    let _answering = answer_for_issues(cx);
    let (events, _subscription) = observe(&board, cx);
    click_row(cx, "d2");
    click(cx, "decisions-issue:NAV-2");
    let origin = events.borrow().navigations[0]
        .origin
        .clone()
        .expect("the selected decision is where Back returns");
    assert_eq!(
        origin.label.as_ref(),
        "Read the relay position after the correction is applied"
    );
    click_row(cx, "d3");
    let returned = cx.update(|window, cx| {
        board.update(cx, |board, cx| board.return_to(&origin.key, window, cx))
    });
    crate::tests::render(cx);
    assert!(returned);
    assert_eq!(selected(&board, cx).as_deref(), Some("decision:d2"));
    assert!(!has(cx, "decisions-page"), "the list shows, not a page");
}
