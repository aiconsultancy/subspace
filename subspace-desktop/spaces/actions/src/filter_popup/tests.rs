use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyElement, Context, IntoElement as _, Render as _, Role, TestAppContext, VisualTestContext,
    Window,
};
use subspace_framework::dashboard::test_support::ToolbarTestAppContext as _;
use subspace_framework::dashboard::{CommandRun, Dashboard as _, ListDashboard as _, prefs};

use crate::{
    Actions,
    model::{Snapshot, StatusFilter},
    test_support::run,
};

fn root(board: &mut Actions, window: &mut Window, cx: &mut Context<Actions>) -> AnyElement {
    board.render(window, cx).into_any_element()
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
}

fn choose_next(field: &'static str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.click(field, cx));
    draw(cx);
    cx.simulate_keystrokes("down enter");
    draw(cx);
}

#[gpui_kit::test]
fn picking_two_filters_keeps_the_panel_open_and_updates_the_list(cx: &mut TestAppContext) {
    cx.update(|cx| prefs::init_file(None, cx));
    let (board, cx) = cx.mount_toolbar(900., root);
    board.update(cx, |board, cx| {
        board.login = Some("ann".into());
        board.repos = vec!["ann/app".into()];
        board.data.insert(
            "ann".into(),
            Snapshot {
                runs: vec![run("ann/app")],
                loaded: ["ann/app: Runs".into()].into(),
                ..Default::default()
            },
        );
        board.rebuild(cx);
        assert_eq!(board.list.rows().count(), 1);
        assert!(board.comfortable_rows());
    });
    draw(cx);
    cx.update(|window, cx| window.click("actions-filters-button", cx));
    draw(cx);
    cx.update(|window, _| {
        let heading = window.find("actions-heading");
        let scope = window.find("actions-scope");
        let filters = window.find("actions-filters-button").bounds();
        let toolbar = window.find("actions-list-toolbar").bounds();
        let status = window.find("actions-Status").bounds();
        let branch = window.find("actions-Branch").bounds();
        assert_eq!(
            filters.left() - toolbar.left(),
            crate::toolbar::LIST_SIDE_INSET
        );
        assert_eq!(heading.label(), Some("Actions"));
        assert_eq!(heading.role(), Some(Role::Heading));
        assert_eq!(scope.label(), Some("GitHub · Signed in as ann"));
        assert_eq!(heading.bounds().left(), filters.left());
        assert!(heading.bounds().right() <= scope.bounds().left());
        assert!(scope.bounds().top() < heading.bounds().bottom());
        assert!(scope.bounds().bottom() < filters.top());
        assert_eq!(status.left(), branch.left());
        assert_eq!(status.right(), branch.right());
        assert!(status.bottom() < branch.top());
    });

    choose_next("actions-Status", cx);
    board.read_with(cx, |board, cx| {
        assert_eq!(board.filters.status, StatusFilter::Active);
        assert_eq!(board.list.rows().count(), 0);
        assert_eq!(prefs::get(cx, "actions.status").as_deref(), Some("Active"));
    });
    choose_next("actions-Branch", cx);
    board.read_with(cx, |board, cx| {
        assert_eq!(board.filters.status, StatusFilter::Active);
        assert_eq!(board.filters.branch.as_deref(), Some("main"));
        assert_eq!(prefs::get(cx, "actions.branch").as_deref(), Some("main"));
    });
    cx.update(|window, cx| window.click("actions-filters-close", cx));
    draw(cx);
    cx.update(|window, _| assert!(window.try_find("actions-filters-panel").is_none()));
}

#[gpui_kit::test]
fn branch_shortcut_focuses_its_picker_and_escape_closes_the_panel(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::keys::bind(cx);
    });
    let (board, cx) = cx.mount_toolbar(900., root);
    cx.update(|window, cx| board.read(cx).focus.clone().focus(window, cx));
    cx.simulate_keystrokes("b");
    draw(cx);
    cx.update(|window, _| assert_eq!(window.find("actions-Branch").focused(), Some(true)));
    cx.simulate_keystrokes("escape");
    draw(cx);
    cx.update(|window, cx| {
        assert!(window.try_find("actions-filters-panel").is_none());
        assert!(board.read(cx).focus.is_focused(window));
    });
}

#[gpui_kit::test]
fn filter_actions_command_opens_the_panel(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (board, cx) = cx.mount_toolbar(900., root);
    let command = board.update(cx, |board, cx| {
        board
            .commands(cx)
            .into_iter()
            .find(|command| command.label == "Filter Actions…")
            .expect("the Actions board offers its filters in Commands")
    });
    let CommandRun::Action(action) = command.run else {
        panic!("the filters command dispatches an action");
    };
    cx.update(|window, cx| {
        board.read(cx).focus.clone().focus(window, cx);
        window.dispatch_action(action, cx);
    });
    draw(cx);
    cx.update(|window, _| assert!(window.try_find("actions-filters-panel").is_some()));
}
