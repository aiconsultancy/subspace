//! The workspace: the navigator, the pane with its tab strip and toolbar,
//! and the inspector with the selection. Text views use the shared list,
//! Flow draws its job graph, and an opened item's tab shows its page.

use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, Context, InteractiveElement as _, IntoElement, KeyContext, ParentElement as _,
    Render, Styled as _, Window, div,
};
use subspace_framework::dashboard::{DASHBOARD_CONTEXT, Dashboard as _, Tokens, list, workspace};

use crate::{Actions, dialogs, inspector, keys, model::View, navigator, open, panel, toolbar};

impl Render for Actions {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        // The shared list asks for its inset without the tokens, so it
        // reads the text edge this frame worked out.
        self.text_edge = workspace::text_edge(&tokens);
        let body = shell(self, window, cx);
        // An element declares context names; `view_keys` is a binding
        // predicate and must not be parsed as one.
        let mut context = KeyContext::new_with_defaults(cx.platform_info().os);
        context.add(DASHBOARD_CONTEXT);
        context.add(Self::ID);
        keys::handlers(
            v_flex()
                .size_full()
                .relative()
                .key_context(context)
                .track_focus(&self.focus),
            cx,
        )
        .bg(tokens.bg)
        .text_color(tokens.fg)
        .child(div().flex_1().min_h_0().child(body))
        .children(self.menu_anchors.view())
        .children(dialogs::render(self, cx))
    }
}

/// The navigator and the inspector, unless hidden, beside the pane.
fn shell(this: &Actions, window: &mut Window, cx: &mut Context<Actions>) -> workspace::Workspace {
    let shell = workspace::Workspace::new("actions-workspace");
    let shell = if this.docks.navigator {
        shell.navigator(navigator::render(this, cx))
    } else {
        shell
    };
    let shell = shell.pane(pane(this, window, cx));
    match inspector::render(this, cx) {
        Some(inspector) => shell.inspector(inspector),
        None => shell,
    }
}

/// The opened item's page while its tab shows; else the view: the job
/// graph for Flow, the shared list for the others.
fn pane(this: &Actions, window: &mut Window, cx: &mut Context<Actions>) -> AnyElement {
    if this.opened().is_some() || this.view == View::Flow {
        let content = match this.opened() {
            Some(opened) => open::page(this, opened, cx),
            None => panel::flow(this, cx),
        };
        return workspace::Pane::new("actions-pane")
            .tabs(open::tab_bar(this, cx))
            .toolbar(toolbar::render(this, cx))
            .child(div().flex_1().min_h_0().child(content))
            .into_any_element();
    }
    let tabs = open::tab_bar(this, cx).into_any_element();
    let bar = toolbar::render(this, cx).into_any_element();
    let head = workspace::pane_head(Some(tabs), Some(bar), cx);
    list::render_under(this, head, window, cx).into_any_element()
}

pub(crate) fn open_selected(this: &Actions, cx: &mut App, copy: bool) {
    if let Some(record) = this.selected() {
        if copy {
            subspace_framework::dashboard::links::copy(record.link.clone(), cx);
        } else {
            subspace_framework::dashboard::links::open(&record.link, cx);
        }
    }
}

#[cfg(test)]
#[path = "view/notes_tests.rs"]
mod notes_tests;

#[cfg(test)]
mod tests {
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{
        AnyElement, Context, Entity, IntoElement as _, Render as _, SharedString, TestAppContext,
        VisualTestContext, Window,
    };
    use subspace_framework::dashboard::test_support::ToolbarTestAppContext as _;
    use subspace_framework::dashboard::workspace::{SpaceDock, SpaceDocks};
    use subspace_framework::dashboard::{Dashboard as _, list, prefs};

    use crate::{
        Actions,
        menus::{self, Picker},
        model::{Snapshot, View},
        test_support::run,
        writes::Operation,
    };

    fn root(board: &mut Actions, window: &mut Window, cx: &mut Context<Actions>) -> AnyElement {
        board.render(window, cx).into_any_element()
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| window.render_frame(cx));
    }

    /// Uses a dock's switch as the shell's status bar does: through the
    /// Space's dock contract.
    fn use_dock_switch(board: &Entity<Actions>, cx: &mut VisualTestContext, dock: SpaceDock) {
        board.update_in(cx, |board, window, cx| board.toggle_dock(dock, window, cx));
        draw(cx);
    }

    /// A drawn board signed in as `ann`, with one finished run in `ann/app`,
    /// wide enough for the navigator, a run row's lanes and the inspector.
    fn mounted(cx: &mut TestAppContext) -> (Entity<Actions>, &mut VisualTestContext) {
        mounted_at(cx, 1400.)
    }

    /// The same board, `width` wide.
    fn mounted_at(
        cx: &mut TestAppContext,
        width: f32,
    ) -> (Entity<Actions>, &mut VisualTestContext) {
        cx.update(|cx| prefs::init_file(None, cx));
        let (board, cx) = cx.mount_toolbar(width, root);
        board.update(cx, |board, cx| {
            board.login = Some("ann".into());
            board.repos = vec!["ann/app".into()];
            board.data.insert(
                "ann".into(),
                Snapshot {
                    runs: vec![run("ann/app")],
                    loaded: ["ann/app: Runs".to_owned()].into(),
                    ..Default::default()
                },
            );
            board.rebuild(cx);
        });
        draw(cx);
        (board, cx)
    }

    // The root once parsed a binding predicate as its key context, which
    // overflowed the stack on the first frame.
    #[gpui_kit::test]
    fn every_view_renders_its_root(cx: &mut TestAppContext) {
        let (board, cx) = cx.mount_toolbar(900., root);
        for view in [View::Runs, View::Deployments, View::Releases, View::Flow] {
            board.update(cx, |board, cx| {
                board.view = view;
                cx.notify();
            });
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            board.read_with(cx, |board, _| assert_eq!(board.view, view));
        }
    }

    #[gpui_kit::test]
    fn the_views_are_navigator_rows_and_a_click_changes_view(cx: &mut TestAppContext) {
        let (board, cx) = mounted(cx);
        cx.update(|window, cx| {
            let runs = window.find("actions-view-Runs");
            let releases = window.find("actions-view-Releases");
            assert_eq!(runs.selected(), Some(true));
            assert_ne!(releases.selected(), Some(true));
            assert!(window.try_find("actions-tabs").is_none(), "no view tabs");
            window.click("actions-view-Releases", cx);
        });
        draw(cx);
        board.read_with(cx, |board, cx| {
            let saved = prefs::get(cx, "actions.view");
            assert_eq!(board.view, View::Releases);
            assert_eq!(saved.as_deref(), Some("Releases"));
        });
        cx.update(|window, _| {
            let runs = window.find("actions-view-Runs");
            let releases = window.find("actions-view-Releases");
            assert_eq!(releases.selected(), Some(true));
            assert_ne!(runs.selected(), Some(true));
        });
    }

    #[gpui_kit::test]
    fn a_workflow_row_selects_its_group_in_the_list(cx: &mut TestAppContext) {
        let (board, cx) = mounted(cx);
        cx.update(|window, cx| {
            let workflow = window.find("actions-group-ann/app · CI");
            assert_eq!(workflow.label(), Some("app · CI"), "the owner is dropped");
            assert_ne!(workflow.selected(), Some(true));
            window.click("actions-group-ann/app · CI", cx);
        });
        draw(cx);
        board.read_with(cx, |board, _| {
            let selected = board.list.selected().map(AsRef::as_ref);
            assert_eq!(selected, Some("ann/app · CI"));
        });
        cx.update(|window, _| {
            let workflow = window.find("actions-group-ann/app · CI");
            assert_eq!(workflow.selected(), Some(true));
        });
    }

    #[gpui_kit::test]
    fn hiding_the_navigator_leaves_the_views_in_a_toolbar_menu(cx: &mut TestAppContext) {
        let (board, cx) = mounted(cx);
        cx.update(|window, _| assert!(window.try_find("actions-picker-View").is_none()));
        let docks = board.read_with(cx, |board, cx| board.docks(cx));
        assert_eq!(docks, SpaceDocks::both(true, true));
        use_dock_switch(&board, cx, SpaceDock::Navigator);
        cx.update(|window, cx| {
            let menu = window.find("actions-picker-View");
            let saved = prefs::get(cx, "actions.navigator");
            assert!(window.try_find("workspace-navigator").is_none());
            assert_eq!(menu.label(), Some("Runs"), "it names the view");
            assert_eq!(saved.as_deref(), Some("hidden"));
        });
        board.update(cx, |board, cx| {
            let views = menus::picker_values(board, Picker::View);
            assert_eq!(views, ["Runs", "Deployments", "Releases", "Flow"]);
            menus::pick(board, Picker::View, "Flow".into(), cx);
            assert_eq!(board.view, View::Flow);
        });
        draw(cx);
        cx.update(|window, _| {
            let menu = window.find("actions-picker-View");
            assert_eq!(menu.label(), Some("Flow"));
        });
        use_dock_switch(&board, cx, SpaceDock::Navigator);
        cx.update(|window, _| {
            let flow = window.find("actions-view-Flow");
            assert!(window.try_find("actions-picker-View").is_none());
            assert_eq!(flow.selected(), Some(true));
        });
    }

    const RUN: &str = "ann/app#notice#run-1";

    /// The lanes of the fixture run's row; a click there is a click on it.
    fn run_row() -> SharedString {
        SharedString::from(format!("actions-lanes:{RUN}"))
    }

    fn select_run(board: &Entity<Actions>, cx: &mut VisualTestContext) {
        board.update(cx, |board, cx| {
            list::update(board, cx, |list| list.select(RUN.into()));
        });
        draw(cx);
    }

    #[gpui_kit::test]
    fn the_inspector_asks_for_a_run_until_one_is_selected(cx: &mut TestAppContext) {
        let (board, cx) = mounted(cx);
        cx.update(|window, _| {
            assert!(window.try_find("actions-inspector-empty").is_some());
            let title = window.find("workspace-inspector-title");
            assert_eq!(title.label(), Some("Run"));
            assert!(
                window.try_find("actions-run-actions").is_none(),
                "no footer"
            );
        });
        select_run(&board, cx);
        cx.update(|window, _| {
            assert!(window.try_find("actions-inspector-empty").is_none());
            let title = window.find("workspace-inspector-title");
            assert_eq!(title.label(), Some("Run 1"));
            let status = window.find("actions-inspector-status");
            assert_eq!(status.label(), Some("Success"));
            assert!(window.try_find("actions-inspector-open").is_some());
        });
    }

    #[gpui_kit::test]
    fn one_click_selects_a_run_and_a_double_click_opens_it_in_a_tab(cx: &mut TestAppContext) {
        let (board, cx) = mounted(cx);
        cx.update(|window, cx| window.click(run_row(), cx));
        draw(cx);
        board.read_with(cx, |board, _| {
            assert_eq!(board.list.selected().map(AsRef::as_ref), Some(RUN));
            assert!(board.open.is_none(), "a click only selects");
        });
        cx.update(|window, cx| window.double_click(run_row(), cx));
        draw(cx);
        board.read_with(cx, |board, _| {
            let open = board
                .open
                .as_ref()
                .map(|open| (open.label.as_str(), open.active));
            assert_eq!(open, Some(("Run 1", true)));
            assert!(board.list.open_row().is_none(), "the list keeps no panel");
        });
        cx.update(|window, cx| {
            assert!(window.try_find("actions-run-page").is_some());
            let mut tabs = window.within("actions-open-tabs");
            assert_eq!(tabs.find(1usize).label(), Some("Run 1"));
            tabs.click(0usize, cx);
        });
        draw(cx);
        cx.update(|window, cx| {
            assert!(
                window.try_find("actions-run-page").is_none(),
                "back to the list"
            );
            window.within("actions-open-tabs").click(1usize, cx);
        });
        draw(cx);
        // A tab shows its close button while it is the current tab.
        cx.update(|window, cx| {
            window
                .within("actions-open-tabs")
                .within(1usize)
                .click("close-tab", cx);
        });
        draw(cx);
        board.read_with(cx, |board, _| assert!(board.open.is_none()));
    }

    #[gpui_kit::test]
    fn enter_and_the_inspectors_open_open_the_selected_run(cx: &mut TestAppContext) {
        let (board, cx) = mounted(cx);
        select_run(&board, cx);
        cx.update(|window, cx| window.click("actions-inspector-open", cx));
        draw(cx);
        board.update(cx, |board, cx| {
            assert!(board.open.as_ref().is_some_and(|open| open.active));
            board.close_tab(cx);
            list::update(board, cx, |list| {
                list.activate();
            });
            assert!(board.open.is_some(), "Enter opens it too");
            board.set_view(View::Releases, cx);
            let open = board.open.as_ref().map(|open| open.active);
            assert_eq!(open, Some(false), "a view shows; the tab stays");
        });
    }

    #[gpui_kit::test]
    fn the_writes_on_a_run_sit_in_the_inspectors_footer(cx: &mut TestAppContext) {
        let (board, cx) = mounted(cx);
        cx.update(|window, _| {
            let trigger = window.find("actions-intent-Trigger");
            assert_eq!(trigger.label(), Some("Trigger…"));
            assert_eq!(trigger.disabled(), Some(true), "no run is selected");
            assert!(window.try_find("actions-intent-Re-run").is_none());
        });
        select_run(&board, cx);
        cx.update(|window, cx| {
            let enabled = |id: &'static str| window.find(id).disabled() != Some(true);
            assert!(enabled("actions-intent-Trigger"));
            assert!(enabled("actions-intent-Re-run"));
            assert!(enabled("actions-intent-Re-run failed"));
            assert!(!enabled("actions-intent-Cancel run"), "it has finished");
            assert!(!enabled("actions-intent-Approve"), "no review waits");
            assert!(!enabled("actions-intent-Reject"));
            let toolbar = window.find("actions-list-toolbar").bounds();
            let footer = window.find("actions-run-actions").bounds();
            assert!(toolbar.right() <= footer.left(), "in the inspector");
            let approve = window.find("actions-intent-Approve").bounds();
            let rerun = window.find("actions-intent-Re-run").bounds();
            assert!(approve.left() < rerun.left() || approve.top() < rerun.top());
            window.click("actions-intent-Re-run", cx);
        });
        cx.run_until_parked();
        board.read_with(cx, |board, _| {
            let pending = board.pending.as_ref().map(|pending| pending.operation);
            assert_eq!(pending, Some(Operation::Rerun));
        });
    }

    #[gpui_kit::test]
    fn a_waiting_review_enables_approve_and_reject(cx: &mut TestAppContext) {
        let (board, cx) = mounted(cx);
        board.update(cx, |board, cx| {
            let mut review = run("ann/app");
            review.key = "ann/app#approval#1#3".into();
            review.status = "waiting".into();
            review.active = true;
            review.approval = Some((3, true));
            if let Some(data) = board.data.get_mut("ann") {
                data.deployments = vec![review];
            }
            board.set_view(View::Deployments, cx);
            list::update(board, cx, |list| list.select("ann/app#approval#1#3".into()));
        });
        draw(cx);
        cx.update(|window, _| {
            let enabled = |id: &'static str| window.find(id).disabled() != Some(true);
            assert!(enabled("actions-intent-Approve"));
            assert!(enabled("actions-intent-Reject"));
            assert!(enabled("actions-intent-Cancel run"), "it is still running");
            let status = window.find("actions-inspector-status");
            assert_eq!(status.label(), Some("Waiting for approval"));
        });
    }

    /// The lanes once crowded out the title: their fixed columns were
    /// wider than the narrowest list.
    #[gpui_kit::test]
    fn a_narrow_list_keeps_room_for_the_run_title(cx: &mut TestAppContext) {
        let (board, cx) = mounted_at(cx, 360.);
        board.update(cx, |board, cx| {
            board.docks.navigator = false;
            board.docks.inspector = false;
            cx.notify();
        });
        draw(cx);
        cx.update(|window, _| {
            // The board is drawn from the window's left edge, 360 wide.
            let width = gpui_kit::px(360.);
            let lanes = window.find(run_row()).bounds();
            let share = crate::lanes::LANES_MAX_SHARE;
            assert!(
                lanes.size.width <= width * share + gpui_kit::px(1.),
                "the lanes take at most their share of the row"
            );
            assert!(lanes.right() <= width, "and stay inside it");
            assert!(
                lanes.left() >= width * (1. - share) - gpui_kit::px(1.),
                "the title keeps the rest"
            );
        });
    }

    #[gpui_kit::test]
    fn hiding_the_inspector_keeps_the_selection_and_its_commands(cx: &mut TestAppContext) {
        let (board, cx) = mounted(cx);
        select_run(&board, cx);
        cx.update(|window, _| assert!(window.try_find("actions-run-actions").is_some()));
        use_dock_switch(&board, cx, SpaceDock::Inspector);
        cx.update(|window, cx| {
            assert!(window.try_find("actions-inspector").is_none());
            let saved = prefs::get(cx, "actions.inspector");
            assert_eq!(saved.as_deref(), Some("hidden"));
        });
        board.update(cx, |board, cx| {
            assert!(board.list.selected().is_some(), "the run stays selected");
            let commands = board.commands(cx);
            let offered = |label: &str| commands.iter().any(|command| command.label == label);
            assert!(offered("Re-run failed…"), "still in Commands");
            assert!(!offered("Cancel run…"), "the run has finished");
            board.show_inspector(cx);
        });
        draw(cx);
        cx.update(|window, _| assert!(window.try_find("actions-run-actions").is_some()));
    }
}
