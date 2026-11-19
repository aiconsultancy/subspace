use std::{cell::RefCell, rc::Rc, time::Duration};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::{SearchView, SearchViewEvent};
use crate::full_text::{Document, Policy, Scope, Service};

fn populated(root: &std::path::Path) -> (Service, Scope) {
    let scope = Scope {
        app: "test-app".into(),
        board: "irc".into(),
        endpoint: "https://example.invalid/test-only".into(),
        tenant: "test-only-tenant".into(),
        account: "test-only-account-a".into(),
    };
    let service = Service::open(
        root.to_owned(),
        scope.app.clone(),
        vec![scope.board.clone()],
    )
    .unwrap();
    service
        .set_policy(
            &scope.board,
            Policy {
                enabled: true,
                ..Policy::default()
            },
        )
        .unwrap();
    service.activate(&scope).unwrap();
    service
        .upsert(
            &scope,
            &[Document {
                id: "test-only-message".into(),
                title: "Retained deployment".into(),
                body: "deployment evidence".into(),
                metadata: String::new(),
                parent_id: Some("test-only-chat".into()),
                kind: "message".into(),
                url: None,
                navigation: "test-only-chat/test-only-message".into(),
                refreshed_at: chrono::Utc::now().timestamp(),
                payload: None,
            }],
        )
        .unwrap();
    (service, scope)
}

fn open(cx: &mut TestAppContext, service: Service) -> (Entity<SearchView>, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let (view, cx) = cx.add_window_view(|window, cx| {
        SearchView::new(
            service,
            vec![("irc".into(), "IRC".into())],
            "irc".into(),
            window,
            cx,
        )
    });
    cx.run_until_parked();
    (view, cx)
}

fn search(view: &Entity<SearchView>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.input
                .update(cx, |input, cx| input.set_value("deployment", window, cx));
            view.submit(window, cx);
        });
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn query_keys_select_and_open_the_revalidated_retained_target(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (service, scope) = populated(root.path());
    let (view, cx) = open(cx, service);
    let events = Rc::new(RefCell::new(Vec::new()));
    let received = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(&view, move |_, event, _| {
            if let SearchViewEvent::Open(hit) = event {
                received.borrow_mut().push(hit.clone());
            }
        })
        .detach()
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.input("deployment", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.press("enter", cx));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.response.as_ref().unwrap().hits.len(), 1);
        assert_eq!(view.selected, None);
    });
    cx.update(|window, cx| {
        window.press("down", cx);
        window.press("up", cx);
    });
    view.read_with(cx, |view, _| assert_eq!(view.selected, Some(0)));
    cx.update(|window, cx| window.press("enter", cx));
    cx.run_until_parked();
    let events = events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].target.scope, scope);
    assert_eq!(
        events[0].document.navigation,
        "test-only-chat/test-only-message"
    );
}

#[gpui_kit::test]
fn disabling_a_source_ignores_invalid_drafts_and_invalidates_visible_results(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let (service, _) = populated(root.path());
    let saved = service.clone();
    let (view, cx) = open(cx, service);
    search(&view, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(view.response.is_some());
            view.editors[0]
                .days
                .update(cx, |input, cx| input.set_value("invalid", window, cx));
            view.save_source(0, false, window, cx);
            assert!(view.response.is_none());
            assert!(view.busy);
        })
    });
    cx.run_until_parked();
    let policy = saved.policy("irc").unwrap();
    assert!(!policy.enabled);
    assert_eq!(policy.retention_days, Policy::default().retention_days);
    view.read_with(cx, |view, _| {
        assert!(!view.busy);
        assert!(view.error.is_none());
        assert!(view.notice.is_some());
        assert!(!view.editors[0].policy.enabled);
    });
}

#[gpui_kit::test]
fn monitor_rejects_results_after_an_account_change_on_the_same_board(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (service, mut scope) = populated(root.path());
    let source = service.clone();
    let (view, cx) = open(cx, service);
    search(&view, cx);
    view.read_with(cx, |view, _| assert!(view.response.is_some()));
    scope.account = "test-only-account-b".into();
    source.activate(&scope).unwrap();
    cx.executor().advance_clock(Duration::from_secs(3));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.current, "irc");
        assert!(view.response.is_none());
        assert_eq!(view.selected, None);
        assert_eq!(
            view.sources[0].account.as_deref(),
            Some("test-only-account-b")
        );
        assert!(
            view.notice
                .as_ref()
                .unwrap()
                .contains("Search sources changed")
        );
    });
}

#[gpui_kit::test]
fn invalidation_rejects_pending_query_and_open_completions(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (service, _) = populated(root.path());
    let (view, cx) = open(cx, service);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.input
                .update(cx, |input, cx| input.set_value("deployment", window, cx));
            view.submit(window, cx);
            assert!(view.loading);
            view.set_current_board("irc", window, cx);
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.response.is_none());
        assert!(!view.loading);
    });
    search(&view, cx);
    let opened = Rc::new(RefCell::new(0));
    let received = opened.clone();
    cx.update(|_, cx| {
        cx.subscribe(&view, move |_, event, _| {
            if matches!(event, SearchViewEvent::Open(_)) {
                *received.borrow_mut() += 1;
            }
        })
        .detach()
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert_eq!(view.response.as_ref().unwrap().hits.len(), 1);
            view.open_result(0, window, cx);
            view.set_current_board("irc", window, cx);
        })
    });
    cx.run_until_parked();
    assert_eq!(*opened.borrow(), 0);
    view.read_with(cx, |view, _| assert!(view.response.is_none()));
}
