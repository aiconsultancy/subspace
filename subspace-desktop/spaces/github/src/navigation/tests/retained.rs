use std::{cell::RefCell, rc::Rc};

use gpui_kit::{App, Entity, TestAppContext, VisualTestContext};
use subspace_client_fleet::{FleetApi, FleetRecord, TokenStore, navigation::OpenCrewDiff};
use subspace_framework::dashboard::full_text::{Hit, Policy, ReadPermit, Service, Target};
use subspace_framework::dashboard::{Dashboard as _, DashboardEvent, ReturnPoint};

use super::{KEY, mount, render, seed};
use crate::search_index::mapping;
use crate::{Github, key_help};

fn install_search(service: &Service, cx: &mut App) {
    let config = subspace_framework::core::config::Config::default();
    cx.set_global(subspace_framework::dashboard::Services {
        credentials: subspace_framework::agent_tools::test_support::credentials(&config),
        config: std::sync::Arc::new(config),
        tools: subspace_framework::dashboard::Catalog::new(vec![]).unwrap(),
        search: Some(service.clone()),
        config_file: "/fixture/config.json".into(),
    });
}

fn retained_hit_fixture(
    cx: &mut TestAppContext,
) -> (
    Entity<Github>,
    &mut VisualTestContext,
    Hit,
    ReadPermit,
    tempfile::TempDir,
) {
    let root = tempfile::tempdir().unwrap();
    let service = Service::open(root.path().into(), "app".into(), vec!["github".into()]).unwrap();
    service
        .set_policy(
            "github",
            Policy {
                enabled: true,
                ..Policy::default()
            },
        )
        .unwrap();
    let scope = mapping::scope("app", "https://api.github.com", "fixture-node").unwrap();
    service.activate(&scope).unwrap();
    let (board, cx) = mount(cx, None);
    board.update(cx, seed);
    let document = board.read_with(cx, |board, _| {
        mapping::document(
            &board.account_data().unwrap().details[KEY],
            chrono::Utc::now().timestamp(),
            "fixture-node",
            "octo-dev",
        )
        .unwrap()
    });
    service.upsert(&scope, &[document]).unwrap();
    let target = Target {
        scope,
        id: KEY.into(),
    };
    let cached = service.read_fenced(&target).unwrap().unwrap();
    let hit = Hit {
        target,
        document: cached.document,
        excerpt: String::new(),
        score: 1.0,
    };
    cx.update(|_, cx| install_search(&service, cx));
    (board, cx, hit, cached.permit, root)
}

#[gpui_kit::test]
fn a_retained_hit_discards_navigation_to_another_pr(cx: &mut TestAppContext) {
    let (board, cx, hit, permit, _root) = retained_hit_fixture(cx);
    board.update_in(cx, |board, window, cx| {
        assert!(board.open_link("https://github.com/octo-org/api/pull/999", cx));
        assert!(board.navigation.is_some());
        assert_eq!(
            board.list.open_row().unwrap().key.as_ref(),
            "octo-org/api#999"
        );
        board.open_indexed_result_fenced(&hit, permit, window, cx);
        assert!(board.navigation.is_none());
        assert_eq!(board.list.open_row().unwrap().key.as_ref(), KEY);
        assert_eq!(board.retained_key.as_deref(), Some(KEY));
        assert_eq!(board.list.rows().count(), 1);
    });
}

#[gpui_kit::test]
fn a_retained_pr_hit_discards_crew_diff_keys_and_back_point(cx: &mut TestAppContext) {
    let (board, cx, hit, permit, _root) = retained_hit_fixture(cx);
    let route = OpenCrewDiff {
        fleet: "example-fleet".into(),
        ship: "example-ship".into(),
        member: "example-crew".into(),
        project: None,
    }
    .navigation()
    .url;
    let returned = Rc::new(RefCell::new(Vec::new()));
    let events = returned.clone();
    let _subscription = cx.update(|_, cx| {
        FleetApi::install_new(
            TokenStore::memory(),
            vec![FleetRecord::new("example-fleet", "Example fleet", None)],
            cx,
        );
        cx.subscribe(&board, move |_, event: &DashboardEvent, _| {
            if let DashboardEvent::Return(point) = event {
                events.borrow_mut().push(point.clone());
            }
        })
    });
    let origin = ReturnPoint {
        board: "irc",
        label: "#example".into(),
        location: Some("room".into()),
    };
    board.update(cx, |board, cx| {
        board.arrived_from(Some(origin.clone()), cx);
        assert!(board.open_link(&route, cx));
    });
    render(cx);
    board.update_in(cx, |board, window, cx| board.close_diff(window, cx));
    assert_eq!(returned.borrow_mut().pop(), Some(origin.clone()));

    board.update(cx, |board, cx| {
        board.arrived_from(Some(origin), cx);
        assert!(board.open_link(&route, cx));
    });
    render(cx);
    board.update_in(cx, |board, window, cx| {
        assert_eq!(board.keys(), crate::CREW_DIFF_KEYS);
        board.open_indexed_result_fenced(&hit, permit, window, cx);
        assert!(board.diff.is_none());
        assert_eq!(board.keys(), key_help(false));
        assert_eq!(board.list.open_row().unwrap().key.as_ref(), KEY);
        // Even the diff's Back handler no longer has the former crew origin.
        board.close_diff(window, cx);
    });
    render(cx);
    cx.simulate_keystrokes("escape");
    board.read_with(cx, |board, _| assert!(board.list.open_row().is_none()));
    assert!(returned.borrow().is_empty());
}
