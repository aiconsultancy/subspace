//! Decision references as drawn and opened by Channels, against both API generations.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{SharedString, TestAppContext, VisualTestContext};
use serde_json::json;
use subspace_api_client::Client;
use subspace_framework::client_test_support::Mock;
use subspace_framework::contracts::decisions::{self, OpenDecision};
use subspace_framework::dashboard::contracts::{self, BoardContracts, Declaration, MessageKey};
use subspace_framework::dashboard::{BoardNavigation, Dashboard as _, DashboardEvent};

use super::{open_with_message, pull_requests::chips, settings, tooltip_text};
use crate::fixtures::channel;
use crate::{Irc, back, hub};

const ID: &str = "D-00042-EXA";
const TEXT: &str = "See D-00042-EXA for the rollout.";
const TITLE: &str = "Roll out the workspace in stages";
const TOOLTIP: &str = "irc-decision-hover-card";
const DECISIONS: BoardContracts = BoardContracts::new(&[Declaration::responds::<OpenDecision>()]);

fn detail() -> String {
    json!({
        "id": "fixture-decision", "identifier": ID, "title": TITLE,
        "status": "accepted", "decidedOn": "2026-11-24", "projectId": "fixture-project",
        "projectKey": "EXA", "issues": {"items": [], "count": 0}, "tags": [],
        "madeByType": "human", "recordedByUserId": "fixture-operator", "revisionNumber": 1,
        "createdAt": "2026-11-24T12:00:00Z", "updatedAt": "2026-11-24T12:00:00Z",
        "decision": "Release in stages", "context": "", "why": "", "implications": "", "alternatives": "",
        "provenance": {"madeByType": "human"},
        "recordedBy": {"userId": "fixture-operator", "displayName": "Fixture operator",
                       "available": true, "kind": "human"}
    }).to_string()
}

fn install(mock: &Mock, cx: &mut gpui_kit::App) -> (gpui_kit::Entity<hub::Hub>, ()) {
    contracts::install(
        vec![
            (crate::ID, Irc::contracts()),
            ("fixture-decisions", DECISIONS),
        ],
        cx,
    );
    let hub = hub::install_for_test(&settings(), cx);
    let client = Client::loopback(&format!("{}/api", mock.base_url()), None).unwrap();
    hub.update(cx, |hub, cx| {
        hub.decision_support.probe(move || Ok(client), cx)
    });
    (hub, ())
}

fn render(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
}

#[gpui_kit::test]
fn a_legacy_api_leaves_the_reference_as_text(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| {
        (
            200,
            json!({"items": [{"id": "fixture-decision"}]}).to_string(),
        )
    });
    let (_, _, (), cx) = open_with_message(cx, |cx| install(&mock, cx), TEXT);
    assert_eq!(mock.requests().len(), 1);
    assert!(chips(cx).is_empty());
}

#[gpui_kit::test]
fn the_chip_previews_and_opens_the_decision_with_its_conversation_return_point(
    cx: &mut TestAppContext,
) {
    let mock = Mock::new(|request| {
        if request.path.ends_with(ID) {
            (200, detail())
        } else {
            (200, json!({"items": [{"identifier": ID}]}).to_string())
        }
    });
    let (board, _, (), cx) = open_with_message(cx, |cx| install(&mock, cx), TEXT);

    let chip: SharedString = chips(cx).into_iter().next().expect("decision chip").into();
    cx.update(|window, cx| window.hover(chip.clone(), cx));
    cx.executor().advance_clock(Duration::from_secs(1));
    render(cx);
    let text = tooltip_text(TOOLTIP, cx);
    for expected in [ID, TITLE, "Accepted"] {
        assert!(text.contains(expected), "{expected} in {text}");
    }

    let route = Rc::new(RefCell::new(None::<BoardNavigation>));
    let _subscription = cx.update(|_, cx| {
        let recorded = route.clone();
        cx.subscribe(&board, move |_, event, _| {
            if let DashboardEvent::Navigate(navigation) = event {
                *recorded.borrow_mut() = Some(navigation.clone());
            }
        })
    });
    cx.update(|window, cx| window.click(chip, cx));
    cx.run_until_parked();
    let navigation = route.borrow_mut().take().expect("chip navigation");
    assert_eq!(navigation.responder, MessageKey::request::<OpenDecision>());
    assert_eq!(decisions::from_link(&navigation.url), Some(ID));
    assert_eq!(navigation.origin, back::place(&channel("#subspace")));
    board.update(cx, |board, cx| {
        let origin = navigation.origin.unwrap();
        assert!(back::show(board, &origin.key, cx));
        assert_eq!(back::location(board), Some(origin));
    });
}
