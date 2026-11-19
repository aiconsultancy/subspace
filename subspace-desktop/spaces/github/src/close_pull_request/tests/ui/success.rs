use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use serde_json::json;
use subspace_framework::client_github::model::PrState;
use subspace_framework::client_test_support::Mock;
use subspace_framework::dashboard::DashboardEvent;

use super::{client, detail, settle, setup};

#[gpui_kit::test]
fn closed_state_wins_over_older_list_and_detail_reads(cx: &mut TestAppContext) {
    let (board, cx) = setup(cx);
    let older = std::time::Instant::now();
    let stale = detail();
    let key = stale.item.key.clone();
    let response = subspace_framework::client_github::model::SectionResponse {
        account: "ann".into(),
        section: "review".into(),
        total: 1,
        items: vec![stale.item.clone()],
    };
    let mock = Mock::new(|request| {
        if request.path == "/graphql" {
            (500, "{}".into())
        } else {
            (200, "{}".into())
        }
    });
    board.update(cx, |board, _| board.client = Some(Arc::new(client(&mock))));
    cx.simulate_keystrokes("x");
    settle(cx);
    cx.simulate_keystrokes(subspace_framework::dashboard::secondary!("enter"));
    settle(cx);
    board.update(cx, |board, cx| {
        let section = board.section;
        let account = board.data.get_mut("ann").unwrap();
        assert_eq!(account.details[&key].item.state, PrState::Closed);
        // Deliver both reads through the production cache application paths.
        account.apply_read_detail(stale.clone(), older);
        account.apply_section(section, response.clone(), older);
        assert_eq!(account.details[&key].item.state, PrState::Closed);
        assert_eq!(account.sections[section].items[0].state, PrState::Closed);
        board.rebuild(cx);
        assert_eq!(board.selected_item().unwrap().state, PrState::Closed);
    });
    // A subsequent read can reflect a deliberate reopen on GitHub.
    let later = std::time::Instant::now();
    board.update(cx, |board, _| {
        let section = board.section;
        let account = board.data.get_mut("ann").unwrap();
        account.apply_read_detail(stale.clone(), later);
        account.apply_section(section, response, later);
        assert_eq!(account.details[&key].item.state, PrState::Open);
        assert_eq!(account.sections[section].items[0].state, PrState::Open);
    });
}

#[gpui_kit::test]
fn close_refreshes_the_original_account_and_emits_the_standard_toast(cx: &mut TestAppContext) {
    let item = detail().item;
    let key = item.key.clone();
    let number = item.number;
    let mock = Mock::new(move |request| {
        if request.path != "/graphql" {
            return (200, "{}".into());
        }
        (
            200,
            json!({"data":{"repository":{"issueOrPullRequest":{
                "__typename":"PullRequest", "id":item.id, "number":number,
                "title":"Refreshed title", "bodyHTML":"<p>Refreshed description</p>",
                "url":item.url, "state":"CLOSED", "repository":{"nameWithOwner":item.repo},
                "createdAt":item.created_at, "updatedAt":item.updated_at,
                "author":{"login":"ann"}
            }}}})
            .to_string(),
        )
    });
    let (board, cx) = setup(cx);
    let messages = Rc::new(RefCell::new(Vec::new()));
    let seen = messages.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&board, move |_, event, _| {
            if let DashboardEvent::Toast(toast) = event {
                seen.borrow_mut().push(toast.text.to_string());
            }
        })
    });
    board.update(cx, |board, _| {
        board.client = Some(Arc::new(client(&mock)));
        board.account = Some("Ann".into());
    });
    cx.update(|window, cx| window.click(("gh-close", 3usize), cx));
    settle(cx);
    // A later account change must not redirect the pending write or its cache update.
    board.update(cx, |board, _| board.account = Some("other".into()));
    cx.simulate_keystrokes(subspace_framework::dashboard::secondary!("enter"));
    settle(cx);
    board.read_with(cx, |board, _| {
        assert!(board.close_confirmation.is_none());
        let refreshed = &board.data["ann"].details[&key];
        assert_eq!(refreshed.item.state, PrState::Closed);
        assert_eq!(refreshed.item.title, "Refreshed title");
        assert_eq!(refreshed.body_html, "<p>Refreshed description</p>");
        assert!(!board.data.contains_key("other"));
    });
    assert_eq!(*messages.borrow(), [format!("Closed #{number}")]);
    assert_eq!(mock.requests().len(), 2);
    board.update_in(cx, |board, window, _| {
        assert!(board.focus.is_focused(window))
    });
}
