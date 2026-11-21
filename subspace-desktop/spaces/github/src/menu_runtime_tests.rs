//! Real board menu entry points, without live requests or an app shell.

use crate::{Github, MenuTarget};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Bounds, Context, Entity, IntoElement, ParentElement as _, Pixels, Render,
    Styled as _, Subscription, TestAppContext, VisualTestContext, Window, div, px,
};
use subspace_framework::dashboard::workspace::{SpaceDock, SpaceDocks, text_edge};
use subspace_framework::dashboard::{Dashboard as _, Tokens};

mod workspace;

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

/// Checks that the open menu hangs from the bottom of trigger `id`.
fn assert_menu_under(cx: &mut VisualTestContext, id: &'static str) {
    cx.update(|window, _| {
        let trigger = window.find(id).bounds();
        assert_menu_edge(window, trigger);
    });
}

/// Hides the navigator, which puts the view menu and the account picker in
/// the pane's toolbar.
fn hide_navigator(board: &Entity<Github>, cx: &mut VisualTestContext) {
    board.update(cx, |board, cx| board.toggle_navigator(cx));
    settle(cx);
}

/// Uses a dock's switch as the shell's status bar does: through the Space's
/// dock contract.
fn use_dock_switch(board: &Entity<Github>, cx: &mut VisualTestContext, dock: SpaceDock) {
    board.update_in(cx, |board, window, cx| board.toggle_dock(dock, window, cx));
    settle(cx);
}

/// The status line the shell shows for the board.
fn status(board: &Entity<Github>, cx: &mut VisualTestContext) -> String {
    board.read_with(cx, |board, cx| {
        board
            .status(cx)
            .map(|line| line.to_string())
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn inline_account_toggle_search_pick_and_focus_return(cx: &mut TestAppContext) {
    let (board, cx) = setup(cx, 1600.);
    hide_navigator(&board, cx);
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
    cx.update(|window, _| assert_eq!(window.find("gh-account-button").focused(), Some(true)));
    assert!(status(&board, cx).starts_with("GitHub · Signed in as test-beta"));
}

#[gpui_kit::test]
fn the_navigator_names_the_space_and_its_one_account_and_the_status_line_carries_it(
    cx: &mut TestAppContext,
) {
    let (root, cx) = mount_accounts(cx, 1600., &["test-alpha"]);
    let board = root.read_with(cx, |root, _| root.board.clone());
    board.read_with(cx, |board, _| {
        use subspace_framework::dashboard::ListDashboard as _;
        assert!(board.comfortable_rows());
    });
    cx.update(|window, cx| {
        assert!(window.try_find("gh-account-button").is_none());
        assert!(window.try_find("github-heading").is_none());
        assert!(window.try_find("github-scope").is_none());
        let footer = window.find("gh-account-line");
        assert_eq!(footer.label(), Some("Signed in to GitHub as test-alpha"));
        let navigator = window.find("workspace-navigator").bounds();
        assert!(footer.bounds().right() <= navigator.right());
        // The toolbar starts on the workspace's text edge, then runs
        // Repositories and Refresh; the dock switches are the status bar's.
        let edge = text_edge(&Tokens::current(cx));
        let toolbar = window.find("github-list-toolbar").bounds();
        let repositories = window.find("gh-repos-button").bounds();
        let refresh = window.find("gh-refresh").bounds();
        assert_eq!(repositories.left() - toolbar.left(), edge);
        assert!(navigator.right() <= toolbar.left());
        assert!(repositories.right() <= refresh.left());
        assert!(window.try_find("workspace-navigator-toggle").is_none());
        let strip = window.find("workspace-tab-strip").bounds();
        assert!(strip.bottom() <= toolbar.top());
    });
    assert!(status(&board, cx).starts_with("GitHub · Signed in as test-alpha"));
    let docks = board.read_with(cx, |board, cx| board.docks(cx));
    assert_eq!(docks, SpaceDocks::both(true, true), "both switches show");
}

#[gpui_kit::test]
fn more_than_one_account_is_a_picker_in_the_navigator_footer_then_after_repositories(
    cx: &mut TestAppContext,
) {
    let (board, cx) = setup(cx, 1600.);
    cx.update(|window, _| {
        assert!(window.try_find("gh-account-line").is_none());
        assert!(window.try_find("gh-view-button").is_none());
        let navigator = window.find("workspace-navigator").bounds();
        let picker = window.find("gh-account-button").bounds();
        let repositories = window.find("gh-repos-button").bounds();
        assert!(picker.right() <= navigator.right());
        assert!(navigator.right() <= repositories.left());
    });
    hide_navigator(&board, cx);
    cx.update(|window, _| {
        assert!(window.try_find("workspace-navigator").is_none());
        let view = window.find("gh-view-button").bounds();
        let repositories = window.find("gh-repos-button").bounds();
        let picker = window.find("gh-account-button").bounds();
        assert!(view.right() <= repositories.left());
        assert!(repositories.right() <= picker.left());
    });
}

#[gpui_kit::test]
fn account_menu_left_edge_follows_the_trigger_after_toolbar_moves(cx: &mut TestAppContext) {
    let (root, cx) = mount(cx, 1600.);
    let board = root.read_with(cx, |root, _| root.board.clone());
    hide_navigator(&board, cx);
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
fn the_account_key_opens_the_menu_at_the_picker(cx: &mut TestAppContext) {
    let (board, cx) = setup(cx, 1600.);
    hide_navigator(&board, cx);
    board.update_in(cx, |board, window, cx| board.focus.focus(window, cx));
    cx.simulate_keystrokes("u");
    settle(cx);
    assert_menu_under(cx, "gh-account-button");
    board.read_with(cx, |board, _| {
        assert!(matches!(
            board.menu.as_ref().map(|(_, target, _)| target),
            Some(MenuTarget::Account)
        ))
    });
    board.update_in(cx, |board, window, cx| board.deactivate(window, cx));
    settle(cx);
    board.read_with(cx, |board, _| assert!(board.menu.is_none()));
    cx.update(|window, _| assert!(window.try_find("menu").is_none()));
}

#[gpui_kit::test]
fn the_navigator_switches_views_and_the_view_menu_stands_in_while_it_is_hidden(
    cx: &mut TestAppContext,
) {
    let (board, cx) = setup(cx, 1600.);
    cx.update(|window, cx| {
        assert_eq!(window.find("gh-view:review").selected(), Some(true));
        window.click("gh-view:mine", cx);
    });
    settle(cx);
    board.read_with(cx, |board, _| assert_eq!(board.section, "mine"));
    cx.update(|window, _| assert_eq!(window.find("gh-view:mine").selected(), Some(true)));
    use_dock_switch(&board, cx, SpaceDock::Navigator);
    board.read_with(cx, |board, cx| {
        assert!(!board.navigator_open);
        let stored = subspace_framework::dashboard::prefs::get(cx, crate::layout::NAVIGATOR_KEY);
        assert_eq!(stored.as_deref(), Some("0"));
    });
    cx.update(|window, cx| window.click("gh-view-button", cx));
    settle(cx);
    assert_menu_under(cx, "gh-view-button");
    board.read_with(cx, |board, _| {
        assert!(matches!(
            board.menu.as_ref().map(|(_, target, _)| target),
            Some(MenuTarget::View)
        ))
    });
    board.update_in(cx, |board, window, cx| board.deactivate(window, cx));
    settle(cx);
    use_dock_switch(&board, cx, SpaceDock::Navigator);
    cx.update(|window, _| assert!(window.try_find("workspace-navigator").is_some()));
}
