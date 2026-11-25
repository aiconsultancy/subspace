//! Native PR actions against an offline GitHub while the detail stays open.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use serde_json::{Value, json};
use subspace_framework::client_github::model::PrState;
use subspace_framework::client_test_support::Mock;
use subspace_framework::dashboard::Dashboard as _;

use super::{client, detail, settle, setup};

#[gpui_kit::test]
fn visible_pr_poll_updates_the_merge_action_and_fences_a_changed_head(cx: &mut TestAppContext) {
    let state = Arc::new(AtomicU8::new(0));
    let mock = github(&state);

    let (board, cx) = setup(cx);
    board.update_in(cx, |board, window, cx| {
        board.client = Some(Arc::new(client(&mock)));
        board.activate(window, cx);
    });
    settle(cx);
    for phase in 0..3 {
        state.store(phase, Ordering::SeqCst);
        cx.executor().advance_clock(Duration::from_secs(25));
        settle(cx);
        assert_eq!(mock.requests().len(), usize::from(phase) + 1);
        match phase {
            0 => cx.update(|window, _| assert_eq!(window.find("gh-merge").disabled(), Some(true))),
            1 => {
                cx.update(|window, cx| {
                    assert_ne!(window.find("gh-merge").disabled(), Some(true));
                    window.click("gh-merge", cx);
                });
                settle(cx);
                cx.update(|window, _| {
                    assert_ne!(window.find("github-merge-submit").disabled(), Some(true));
                });
            }
            _ => cx.update(|window, _| {
                assert_eq!(window.find("github-merge-submit").disabled(), Some(true));
            }),
        }
    }
    cx.update(|window, cx| window.click("github-merge-cancel", cx));
    settle(cx);
    cx.update(|window, cx| window.click("gh-merge", cx));
    settle(cx);
    cx.update(|window, cx| window.click("github-merge-submit", cx));
    settle(cx);
    board.read_with(cx, |board, _| {
        assert_eq!(board.selected_item().unwrap().state, PrState::Merged);
    });
    let requests = mock.requests();
    assert_eq!(requests.iter().filter(|r| r.method == "PUT").count(), 1);
    let merge = requests.iter().find(|r| r.method == "PUT").unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&merge.body).unwrap(),
        json!({"sha": "b".repeat(40), "merge_method": "squash"})
    );

    board.update_in(cx, |board, window, cx| board.deactivate(window, cx));
    cx.executor().advance_clock(Duration::from_secs(50));
    settle(cx);
    assert_eq!(mock.requests().len(), requests.len());
}

fn github(state: &Arc<AtomicU8>) -> Mock {
    let observed = state.clone();
    let item = detail().item;
    Mock::new(move |request| {
        if request.body.contains("ViewerBound") {
            return (
                200,
                json!({"data":{"viewer":{"id":"fixture-ann","login":"ann"}}}).to_string(),
            );
        }
        if request.method == "PUT" {
            observed.store(3, Ordering::SeqCst);
            return (200, json!({ "merged": true }).to_string());
        }
        let phase = observed.load(Ordering::SeqCst);
        let body = json!({
            "__typename": "PullRequest", "id": item.id, "number": item.number,
            "url": item.url, "repository": {"nameWithOwner": item.repo},
            "title": item.title, "createdAt": item.created_at, "updatedAt": item.updated_at,
            "bodyHTML": "", "baseRefName": "main", "viewerCanReact": true,
            "state": if phase == 3 { "MERGED" } else { "OPEN" },
            "mergeable": "MERGEABLE",
            "mergeStateStatus": if phase == 0 { "BLOCKED" } else { "CLEAN" },
            "headRefOid": if phase < 2 { "a".repeat(40) } else { "b".repeat(40) },
        });
        (
            200,
            json!({"data":{"repository":{
                "viewerPermission": "WRITE", "squashMergeAllowed": true, "issueOrPullRequest": body
            }}})
            .to_string(),
        )
    })
}
