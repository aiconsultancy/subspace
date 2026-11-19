use gpui_kit::{App, TestAppContext};
use subspace_framework::dashboard::Dashboard as _;
use subspace_framework::dashboard::full_text::{Hit, Policy, Service, Target};

use super::{KEY, mount, seed};
use crate::search_index::mapping;

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

#[gpui_kit::test]
fn a_retained_hit_discards_navigation_to_another_pr(cx: &mut TestAppContext) {
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
    board.update_in(cx, |board, window, cx| {
        assert!(board.open_link("https://github.com/octo-org/api/pull/999", cx));
        assert!(board.navigation.is_some());
        assert_eq!(
            board.list.open_row().unwrap().key.as_ref(),
            "octo-org/api#999"
        );
        board.open_indexed_result_fenced(&hit, cached.permit, window, cx);
        assert!(board.navigation.is_none());
        assert_eq!(board.list.open_row().unwrap().key.as_ref(), KEY);
        assert_eq!(board.retained_key.as_deref(), Some(KEY));
        assert_eq!(board.list.rows().count(), 1);
    });
}
