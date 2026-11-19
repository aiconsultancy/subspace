//! Real board menu entry points, without live requests or an app shell.

use crate::model::sections_of;
use crate::{Account, Github, MenuTarget, SectionData, sample};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Bounds, Context, Entity, IntoElement, ParentElement as _, Pixels, Render,
    Role, Styled as _, Subscription, TestAppContext, VisualTestContext, Window, div, px,
};
use std::sync::Arc;
use subspace_framework::client_github::{
    Client,
    model::{GhItemDetail, GhUser},
};
use subspace_framework::dashboard::Dashboard as _;

struct BoardWindow {
    board: Entity<Github>,
    width: f32,
    left: f32,
    _subscription: Subscription,
}

impl Render for BoardWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            div()
                .ml(px(self.left))
                .w(px(self.width))
                .h_full()
                .flex()
                .flex_col()
                .child(self.board.clone()),
        )
    }
}

fn mount(cx: &mut TestAppContext, width: f32) -> (Entity<BoardWindow>, &mut VisualTestContext) {
    mount_accounts(cx, width, &["test-alpha", "test-beta"])
}

fn mount_accounts<'a>(
    cx: &'a mut TestAppContext,
    width: f32,
    accounts: &[&str],
) -> (Entity<BoardWindow>, &'a mut VisualTestContext) {
    let accounts: Vec<String> = accounts.iter().map(|login| (*login).to_owned()).collect();
    cx.update(|cx| {
        gpui_kit::init(cx);
        subspace_framework::dashboard::prefs::init_file(None, cx);
        subspace_framework::dashboard::bind_keys(cx);
        Github::bind_keys(cx);
    });
    let (root, cx) = cx.add_window_view(|window, cx| {
        let board = cx.new(|cx| {
            let mut board = Github::new(window, cx);
            board.client = None;
            board.started = true;
            board.accounts = accounts;
            board.account = Some("test-alpha".into());
            board.rebuild(cx);
            board
        });
        let subscription = cx.observe(&board, |_, _, cx| cx.notify());
        BoardWindow {
            board,
            width,
            left: 0.,
            _subscription: subscription,
        }
    });
    settle(cx);
    (root, cx)
}

fn setup(cx: &mut TestAppContext, width: f32) -> (Entity<Github>, &mut VisualTestContext) {
    let (root, cx) = mount(cx, width);
    let board = root.read_with(cx, |root, _| root.board.clone());
    (board, cx)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
}

fn assert_menu_edge(window: &mut Window, trigger: Bounds<Pixels>) {
    let scale = window.scale_factor();
    let popup = window.find("menu").bounds();
    let popup_edge = (f32::from(popup.top()) * scale).round();
    let trigger_edge = (f32::from(trigger.bottom()) * scale).round();
    assert_eq!(
        popup_edge, trigger_edge,
        "trigger={trigger:?}, popup={popup:?}, scale={scale}"
    );
}

#[gpui_kit::test]
fn inline_account_toggle_search_pick_and_focus_return(cx: &mut TestAppContext) {
    let (board, cx) = setup(cx, 1600.);
    cx.update(|window, cx| window.click("gh-account-button", cx));
    settle(cx);
    cx.update(|window, _| {
        assert_eq!(
            window.find("menu").bounds().top(),
            window.find("gh-account-button").bounds().bottom()
        )
    });
    cx.update(|window, cx| window.click("gh-account-button", cx));
    settle(cx);
    board.read_with(cx, |board, _| assert!(board.menu.is_none()));
    cx.update(|window, cx| window.click("gh-account-button", cx));
    settle(cx);
    cx.simulate_keystrokes("b e t a enter");
    settle(cx);
    board.read_with(cx, |board, _| {
        assert_eq!(board.account.as_deref(), Some("test-beta"));
        assert!(board.menu.is_none());
    });
    cx.update(|window, _| {
        assert_eq!(window.find("gh-account-button").focused(), Some(true));
        assert_eq!(
            window.find("github-scope").label(),
            Some("GitHub · Signed in as test-beta")
        );
    });
}

#[gpui_kit::test]
fn the_header_names_the_space_and_its_one_account_above_aligned_controls(cx: &mut TestAppContext) {
    let (root, cx) = mount_accounts(cx, 1600., &["test-alpha"]);
    let board = root.read_with(cx, |root, _| root.board.clone());
    board.read_with(cx, |board, _| {
        use subspace_framework::dashboard::ListDashboard as _;
        assert!(board.comfortable_rows());
    });
    cx.update(|window, _| {
        assert!(window.try_find("gh-account-button").is_none());
        let heading = window.find("github-heading");
        let scope = window.find("github-scope");
        assert_eq!(heading.label(), Some("Pull requests"));
        assert_eq!(heading.role(), Some(Role::Heading));
        assert_eq!(scope.label(), Some("GitHub · Signed in as test-alpha"));
        let tabs = window.find("overflow-unit:gh-section-tabs").bounds();
        let repositories = window.find("gh-repos-button").bounds();
        let toolbar = window.find("github-list-toolbar").bounds();
        assert_eq!(tabs.left() - toolbar.left(), crate::LIST_SIDE_INSET);
        assert_eq!(heading.bounds().left(), tabs.left());
        assert!(heading.bounds().right() <= scope.bounds().left());
        assert!(scope.bounds().top() < heading.bounds().bottom());
        assert!(scope.bounds().bottom() <= tabs.top());
        assert!(tabs.right() <= repositories.left());
        assert!(window.try_find("gh-account-label").is_none());
    });
}

#[gpui_kit::test]
fn more_than_one_account_is_a_picker_after_repositories(cx: &mut TestAppContext) {
    let (_, cx) = mount(cx, 1600.);
    cx.update(|window, _| {
        assert!(window.try_find("gh-account-label").is_none());
        let picker = window.find("gh-account-button").bounds();
        let repositories = window.find("gh-repos-button").bounds();
        assert!(repositories.right() <= picker.left());
    });
}

#[gpui_kit::test]
fn account_menu_left_edge_follows_the_trigger_after_toolbar_moves(cx: &mut TestAppContext) {
    let (root, cx) = mount(cx, 1600.);
    let initial = cx.update(|window, _| window.find("gh-account-button").bounds().left());
    for left in [90., 310.] {
        root.update(cx, |root, cx| {
            root.left = left;
            cx.notify();
        });
        settle(cx);
        cx.update(|window, cx| window.click("gh-account-button", cx));
        settle(cx);
        cx.update(|window, _| {
            let trigger = window.find("gh-account-button").bounds();
            let popup = window.find("menu").bounds();
            assert_eq!(trigger.left(), initial + px(left));
            assert_eq!(
                popup.left(),
                trigger.left(),
                "popup={popup:?}, trigger={trigger:?}"
            );
            assert_menu_edge(window, trigger);
        });
        cx.simulate_keystrokes("escape");
        settle(cx);
        cx.update(|window, _| assert!(window.try_find("menu").is_none()));
    }
}

#[gpui_kit::test]
fn hidden_account_keyboard_and_native_overflow_share_chevron_host(cx: &mut TestAppContext) {
    let (board, cx) = setup(cx, 100.);
    board.update_in(cx, |board, window, cx| board.focus.focus(window, cx));
    cx.simulate_keystrokes("u");
    settle(cx);
    cx.update(|window, _| {
        assert!(window.try_find("gh-account-button").is_none());
        assert_eq!(
            window.find("menu").bounds().top(),
            window.find("overflow-toolbar-button").bounds().bottom()
        );
    });
    cx.simulate_keystrokes("escape");
    settle(cx);
    cx.update(|window, _| assert_eq!(window.find("overflow-toolbar-button").focused(), Some(true)));
    // The account is the last unit, after the section tabs and Repositories.
    let account_entry = board.read_with(cx, |board, _| sections_of(board.kind()).count() + 1);
    cx.update(|window, cx| {
        window.click("overflow-toolbar-button", cx);
        window.click(account_entry, cx);
    });
    settle(cx);
    cx.update(|window, _| {
        assert!(window.try_find("popup-menu").is_none());
        assert!(window.try_find("menu").is_some());
    });
    board.update_in(cx, |board, window, cx| board.deactivate(window, cx));
    settle(cx);
    board.read_with(cx, |board, _| assert!(board.menu.is_none()));
    cx.update(|window, _| assert!(window.try_find("menu").is_none()));
}

fn load_detail(board: &mut Github, cx: &mut Context<Github>) {
    let item = sample::items().remove(0);
    let key = item.key.clone();
    let detail = GhItemDetail {
        item: item.clone(),
        body_html: String::new(),
        reviewers: Vec::new(),
        check_runs: Vec::new(),
        events: Vec::new(),
        viewer_can_review: true,
        viewer_can_comment: true,
        changed_files: Some(0),
    };
    let mut account = Account::default();
    account.sections.insert(
        board.section,
        SectionData {
            total: 1,
            items: vec![item.clone()],
            at: 0,
        },
    );
    account.details.insert(key.clone(), detail);
    account.assignable.insert(
        item.repo.to_lowercase(),
        vec![GhUser {
            login: "test-assignee".into(),
            name: None,
        }],
    );
    board.data.insert("test-alpha".into(), account);
    let config = subspace_framework::core::config::Config::default();
    board.client = Some(Arc::new(
        Client::new(subspace_framework::agent_tools::test_support::credentials(
            &config,
        ))
        .with_api_url("http://127.0.0.1:0"),
    ));
    board.rebuild(cx);
    board.list.reveal(&key);
    cx.notify();
}

#[gpui_kit::test]
fn both_assignee_buttons_use_their_own_measured_trigger_and_keyboard_uses_top(
    cx: &mut TestAppContext,
) {
    let (board, cx) = setup(cx, 1600.);
    board.update(cx, load_detail);
    settle(cx);
    for place in ["top", "bottom"] {
        let id = ("gh-assign", place.len());
        cx.update(|window, cx| window.click(id, cx));
        settle(cx);
        let anchor = board.read_with(cx, |board, cx| match place {
            "bottom" => board.assignee_bottom_anchor.read(cx).bounds(),
            _ => board.assignee_anchor.read(cx).bounds(),
        });
        cx.update(|window, _| assert_menu_edge(window, anchor));
        board.read_with(cx, |board, _| {
            assert!(matches!(
                board.menu.as_ref().map(|(_, target, _)| target),
                Some(MenuTarget::Assignees(_))
            ))
        });
        cx.simulate_keystrokes("escape");
        settle(cx);
        cx.update(|window, _| assert_eq!(window.find(id).focused(), Some(true)));
    }
    board.update_in(cx, |board, window, cx| board.focus.focus(window, cx));
    cx.simulate_keystrokes("a");
    settle(cx);
    let anchor = board.read_with(cx, |board, cx| board.assignee_anchor.read(cx).bounds());
    cx.update(|window, _| assert_menu_edge(window, anchor));
    board.update_in(cx, |board, window, cx| board.deactivate(window, cx));
    settle(cx);
    board.read_with(cx, |board, _| assert!(board.menu.is_none()));
}
