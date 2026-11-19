//! Stored choices, quit warnings, status, Reload, Help and the menu and
//! palette actions.

use gpui_kit::{AppContext as _, TestAppContext};
use subspace_dashboard::dock_panel::{BottomSpan, DockPanelEvent, Placement};

use super::super::terminal_dock::Change;
use super::super::terminal_fixture;
use super::{PANEL_ID, Shell};

#[gpui_kit::test]
fn panel_requests_apply_at_once_and_survive_a_restart(cx: &mut TestAppContext) {
    let shell = Shell::open(cx, &[(crate::terminal::OPEN_KEY, "1")]);
    let panel = shell
        .window
        .update(cx, |app, _, _| terminal_fixture::panel(app))
        .unwrap();
    for event in [
        DockPanelEvent::Move(Placement::Right),
        DockPanelEvent::SetBottomSpan(BottomSpan::FullWidth),
        DockPanelEvent::SetLabels(true),
    ] {
        panel.update(cx, |_, cx| cx.emit(event));
    }
    shell.settle(cx);
    let layout = panel.read_with(cx, |panel, _| panel.layout);
    assert_eq!(layout.placement, Placement::Right);
    assert_eq!(layout.span, BottomSpan::FullWidth);
    assert!(layout.labels, "the header shows its button text");
    let state = std::fs::read_to_string(shell.root.path().join("state.json")).unwrap();
    assert!(state.contains("terminal.button-labels"), "saved at once");

    let root = shell.root;
    let reopened = Shell::open_in(cx, root, &[]);
    let panel = reopened
        .window
        .update(cx, |app, _, _| terminal_fixture::panel(app))
        .unwrap();
    let layout = panel.read_with(cx, |panel, _| panel.layout);
    assert_eq!(layout.placement, Placement::Right);
    assert_eq!(layout.span, BottomSpan::FullWidth);
    assert!(layout.labels && !layout.maximized);
    assert_eq!(
        panel.read_with(cx, |panel, _| panel.shown),
        1,
        "restored open"
    );
    panel.update(cx, |_, cx| cx.emit(DockPanelEvent::Hide));
    reopened.settle(cx);
    assert!(reopened.bounds(cx, PANEL_ID).is_none());
}

#[gpui_kit::test]
fn the_quit_confirmation_names_busy_terminal_programs(cx: &mut TestAppContext) {
    let shell = Shell::open(
        cx,
        &[
            (crate::terminal::OPEN_KEY, "1"),
            (crate::settings::CONFIRM_QUIT_KEY, "true"),
        ],
    );
    let panel = shell
        .window
        .update(cx, |app, _, _| terminal_fixture::panel(app))
        .unwrap();
    panel.update(cx, |panel, _| panel.warnings = vec!["vim".into()]);
    let warnings = shell
        .window
        .update(cx, |app, _, cx| {
            app.request_quit(cx);
            app.quit_warnings.clone()
        })
        .unwrap();
    assert_eq!(warnings, Some(vec!["vim".into()]));
}

#[gpui_kit::test]
fn the_terminal_status_reload_and_help_follow_the_focused_terminal(cx: &mut TestAppContext) {
    let shell = Shell::open(cx, &[(crate::terminal::OPEN_KEY, "1")]);
    let panel = shell
        .window
        .update(cx, |app, _, _| terminal_fixture::panel(app))
        .unwrap();
    shell
        .window
        .update(cx, |app, window, cx| {
            assert_eq!(
                app.status_facts(cx).status.as_deref(),
                Some("⚠ fixture warning")
            );
            app.change_terminal(Change::Toggle, window, cx);
            app.change_terminal(Change::Toggle, window, cx);
            app.reload(&super::super::Reload, window, cx);
            app.toggle_help(&super::super::ToggleHelp, window, cx);
            let (markdown, _) = app.terminal_help(cx).unwrap();
            assert_eq!(markdown, terminal_fixture::HELP);
        })
        .unwrap();
    assert_eq!(panel.read_with(cx, |panel, _| panel.reloads), 1);
}

#[gpui_kit::test]
fn menu_and_palette_actions_move_the_terminal_and_span_it(cx: &mut TestAppContext) {
    let shell = Shell::open(cx, &[(crate::terminal::OPEN_KEY, "1")]);
    let panel = shell
        .window
        .update(cx, |app, _, _| terminal_fixture::panel(app))
        .unwrap();
    let dispatch = |cx: &mut TestAppContext, action: Box<dyn gpui_kit::Action>| {
        cx.update_window(shell.window.into(), |_, window, cx| {
            window.dispatch_action(action, cx);
        })
        .unwrap();
        shell.settle(cx);
    };
    dispatch(cx, Box::new(super::super::MoveTerminalLeft));
    assert_eq!(
        panel.read_with(cx, |panel, _| panel.layout.placement),
        Placement::Left
    );
    dispatch(cx, Box::new(super::super::MoveTerminalToBottom));
    dispatch(cx, Box::new(super::super::ToggleTerminalFullWidth));
    let layout = panel.read_with(cx, |panel, _| panel.layout);
    assert_eq!(layout.placement, Placement::Bottom);
    assert_eq!(layout.span, BottomSpan::FullWidth);
    dispatch(cx, Box::new(super::super::MoveTerminalRight));
    assert_eq!(
        shell.stored(cx, crate::terminal::PLACEMENT_KEY).as_deref(),
        Some("right")
    );
}
