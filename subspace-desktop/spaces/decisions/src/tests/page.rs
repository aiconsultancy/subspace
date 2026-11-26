//! A decision opened as a page: how it opens and closes, its banner, its
//! sections and the facts beside it.

use gpui_kit::TestAppContext;
use subspace_framework::dashboard::Dashboard as _;

use crate::tests::{click, double_click_row, has, label, mount, press, rows, select, selected};

#[gpui_kit::test]
fn enter_opens_the_decision_as_a_page_and_escape_closes_it(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    select(&board, "d2", cx);
    press(cx, "enter");
    assert!(has(cx, "decisions-page"));
    assert!(
        !has(cx, "decisions-open"),
        "the inspector holds the facts now"
    );
    assert_eq!(
        label(cx, "decisions-supersedes-banner").as_deref(),
        Some("Supersedes Read the relay position before each burn")
    );
    for section in ["Decision", "Context", "Why", "Implications"] {
        assert!(has(cx, format!("decisions-section:{section}")), "{section}");
    }
    assert!(
        !has(cx, "decisions-section:Alternatives"),
        "an empty section is left out"
    );
    assert_eq!(
        label(cx, "inspector-fact:Revision").as_deref(),
        Some("Revision: 1")
    );
    let accepted = label(cx, "inspector-fact:Accepted").unwrap_or_default();
    assert!(accepted.ends_with(" by local-operator"), "{accepted}");
    press(cx, "escape");
    assert!(!has(cx, "decisions-page"));
    assert_eq!(selected(&board, cx).as_deref(), Some("decision:d2"));
}

#[gpui_kit::test]
fn a_double_click_opens_the_page_in_its_own_tab(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    double_click_row(cx, "d3");
    assert!(has(cx, "decisions-page"));
    assert_eq!(
        label(cx, "decisions-page-heading").as_deref(),
        Some("Keep the last ten headings in the console")
    );
    let place = cx
        .update(|_, cx| board.read(cx).return_location(cx))
        .unwrap();
    assert_eq!(place.key.as_ref(), "decision:0:d3");
    // The list's tab shows the list and keeps the page's tab.
    board.update(cx, |board, cx| board.show_tab(0, cx));
    crate::tests::render(cx);
    assert!(!has(cx, "decisions-page"));
    assert!(cx.update(|_, cx| board.read(cx).page.is_some()));
    assert_eq!(rows(&board, cx).len(), 5);
}

#[gpui_kit::test]
fn a_superseded_decision_says_so_and_opens_the_one_that_superseded_it(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    board.update(cx, |board, cx| board.open_decision("d6", cx));
    crate::tests::render(cx);
    assert_eq!(
        label(cx, "decisions-superseded-banner").as_deref(),
        Some("Superseded by Read the relay position after the correction is applied")
    );
    click(cx, "decisions-superseded-banner-open");
    assert_eq!(
        label(cx, "decisions-page-heading").as_deref(),
        Some("Read the relay position after the correction is applied")
    );
    assert!(has(cx, "decisions-supersedes-banner"));
}

#[gpui_kit::test]
fn a_decision_that_is_gone_says_so(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    board.update(cx, |board, cx| board.open_decision("missing", cx));
    crate::tests::render(cx);
    assert!(has(cx, "decisions-page-gone"));
}

#[gpui_kit::test]
fn a_link_reads_by_reference_and_back_restores_its_source(cx: &mut TestAppContext) {
    use std::{cell::RefCell, rc::Rc};
    use subspace_framework::dashboard::{DashboardEvent, ReturnPoint};

    let (board, cx) = mount(cx);
    select(&board, "d3", cx);
    let origin = ReturnPoint {
        board: "irc",
        label: "#ops".into(),
        location: Some("ops-buffer".into()),
    };
    let returned = Rc::new(RefCell::new(None));
    let _subscription = cx.update(|_, cx| {
        let received = returned.clone();
        cx.subscribe(&board, move |_, event: &DashboardEvent, _| {
            if let DashboardEvent::Return(point) = event {
                *received.borrow_mut() = Some(point.clone());
            }
        })
    });
    board.update(cx, |board, cx| {
        board.arrived_from(Some(origin.clone()), cx);
        assert!(board.open_link("subspace-decision:d-00002-nav", cx));
    });
    crate::tests::render(cx);
    assert_eq!(
        label(cx, "decisions-page-reference").as_deref(),
        Some("D-00002-NAV")
    );
    click(cx, "decisions-page-back");
    assert_eq!(returned.borrow().as_ref(), Some(&origin));
    assert!(!has(cx, "decisions-page"));
    assert_eq!(selected(&board, cx).as_deref(), Some("decision:d3"));
}
