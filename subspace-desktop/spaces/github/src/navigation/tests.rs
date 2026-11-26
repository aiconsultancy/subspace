use std::sync::Arc;

use gpui_kit::{AppContext as _, Context, Entity, EventEmitter, TestAppContext, VisualTestContext};
use subspace_framework::client_github::{Client, model::GhItemDetail};
use subspace_framework::contracts::github::OpenPullRequest;
use subspace_framework::dashboard::contracts::{self, BoardContracts, Declaration, Role};
use subspace_framework::dashboard::list::ListDashboard as _;
use subspace_framework::dashboard::{Dashboard as _, DashboardEvent};

use crate::{Account, Github, SectionData, sample};

mod drafts;
mod lifecycle;
mod loading;
mod references;
mod retained;

const URL: &str = "https://github.com/octo-org/api/pull/421";
const KEY: &str = "octo-org/api#421";
const REQUESTER: &str = "fixture-requester";
const REQUESTS: BoardContracts = BoardContracts::new(&[Declaration::requests::<OpenPullRequest>()]);

fn mount(
    cx: &mut TestAppContext,
    client: Option<Arc<Client>>,
) -> (Entity<Github>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        subspace_framework::dashboard::prefs::init_file(None, cx);
        subspace_framework::dashboard::bind_keys(cx);
        Github::bind_keys(cx);
    });
    cx.add_window_view(move |window, cx| {
        let mut board = Github::new(window, cx);
        board.started = true;
        board.account = Some("octo-dev".into());
        board.accounts = vec!["octo-dev".into(), "other-account".into()];
        board.client = client;
        board.data.insert("octo-dev".into(), Account::default());
        board.rebuild(cx);
        board
    })
}

fn seed(board: &mut Github, cx: &mut Context<Github>) {
    let item = sample::items()
        .into_iter()
        .find(|item| item.key == KEY)
        .unwrap();
    let account = board.data.get_mut("octo-dev").unwrap();
    account.sections.insert(
        board.section,
        SectionData {
            total: 1,
            items: vec![item.clone()],
            at: 0,
        },
    );
    account.details.insert(
        KEY.into(),
        GhItemDetail {
            item,
            body_html: "<p>Cached pull request</p>".into(),
            reviewers: vec![],
            check_runs: vec![],
            events: vec![],
            viewer_can_review: true,
            viewer_can_comment: true,
            changed_files: Some(1),
            merge_target: None,
            viewer_approved: false,
        },
    );
    board.rebuild(cx);
}

fn render(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

#[gpui_kit::test]
fn known_pr_opens_the_normal_panel_and_unfolds_its_repository(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx, None);
    board.update(cx, seed);
    board.update(cx, |board, cx| {
        board.list.toggle(&"repo:octo-org/api".into());
        assert!(board.open_link(URL, cx));
        assert_eq!(board.list.open_row().unwrap().key.as_ref(), KEY);
        assert!(!board.list.is_folded("repo:octo-org/api"));
        assert_eq!(board.account.as_deref(), Some("octo-dev"));
        assert!(
            board
                .detail(KEY)
                .unwrap()
                .props
                .iter()
                .any(|(name, _)| name == "State")
        );
        assert!(board._detail_task.is_none(), "fresh detail is reused");
    });
    render(cx);
    board.update_in(cx, |board, window, cx| board.focus.focus(window, cx));
    cx.simulate_keystrokes("escape");
    board.read_with(cx, |board, _| assert!(board.list.open_row().is_none()));
}

#[gpui_kit::test]
fn invalid_links_do_not_change_the_current_panel(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx, None);
    board.update(cx, seed);
    board.update(cx, |board, cx| {
        assert!(board.open_link(URL, cx));
        for invalid in [
            "https://github.com/octo-org/api/issues/421",
            "http://github.com/o/r/pull/1",
            "https://github.com/o/r/pull/0",
        ] {
            assert!(!board.open_link(invalid, cx));
            assert_eq!(board.list.open_row().unwrap().key.as_ref(), KEY);
        }
    });
}

#[gpui_kit::test]
fn case_variants_reuse_the_loaded_row(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx, None);
    board.update(cx, seed);
    board.update(cx, |board, cx| {
        assert!(board.open_link("https://github.com/OCTO-ORG/API/pull/421", cx));
        assert_eq!(board.list.open_row().unwrap().key.as_ref(), KEY);
        assert_eq!(board.list.rows().count(), 1);
        assert!(board._detail_task.is_none());
    });
}

#[gpui_kit::test]
fn initial_account_selection_keeps_navigation_and_an_account_switch_clears_it(
    cx: &mut TestAppContext,
) {
    let (board, cx) = mount(cx, None);
    board.update(cx, |board, cx| {
        board.account = None;
        assert!(board.open_link(URL, cx));
        board.set_account("octo-dev".into(), cx);
        assert_eq!(board.list.open_row().unwrap().key.as_ref(), KEY);
        board.set_account("other-account".into(), cx);
        assert!(board.navigation.is_none());
        assert!(board.list.open_row().is_none());
    });
}

struct Requester;
impl EventEmitter<DashboardEvent> for Requester {}

#[gpui_kit::test]
fn requester_navigation_routes_through_the_declared_broker_role(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx, None);
    board.update(cx, seed);
    let requester = cx.new(|_| Requester);
    cx.update(|_, cx| {
        contracts::install(
            vec![(REQUESTER, REQUESTS), (Github::ID, Github::contracts())],
            cx,
        )
    });
    let _subscription = board.update(cx, |_, cx| {
        cx.subscribe(&requester, |board, _, event, cx| {
            if let DashboardEvent::Navigate(link) = event {
                assert_eq!(
                    contracts::navigation_target(REQUESTER, link.responder, cx),
                    Some(Github::ID)
                );
                assert!(board.open_link(&link.url, cx));
            }
        })
    });
    let link = contracts::github::pull_request_navigation(URL).unwrap();
    assert!(Github::contracts().declares(Role::Responds, link.responder));
    requester.update(cx, |_, cx| cx.emit(DashboardEvent::Navigate(link.clone())));
    cx.run_until_parked();
    board.read_with(cx, |board, _| {
        assert_eq!(board.list.open_row().unwrap().key.as_ref(), KEY)
    });
    cx.update(|_, cx| {
        assert!(contracts::navigation_target("undeclared", link.responder, cx).is_none());
        contracts::install(vec![(REQUESTER, REQUESTS)], cx);
        assert!(contracts::navigation_target(REQUESTER, link.responder, cx).is_none());
    });
    assert!(
        contracts::github::pull_request_navigation("https://github.com/o/r/issues/1").is_none()
    );
}
