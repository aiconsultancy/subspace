//! The keyboard and outside routes while the sign-in dialog shows: Tab and
//! Shift-Tab stay on its controls in every state, focus never lands behind
//! it, the quit confirmation still answers the keyboard, the shell's own
//! entry points do nothing and a notification waits until sign-in.

use gpui_kit::WindowHandle;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};

use super::SubspaceApp;
use super::cover_tests::{gated, settle};

const GATE: [&str; 5] = [
    "sign-in-gate-quit",
    "sign-in-gate-sign-in",
    "sign-in-gate-retry",
    "sign-in-gate-cancel",
    "sign-in-gate-reopen",
];

/// The id of the gate control holding focus, if one does.
fn focused(window: WindowHandle<SubspaceApp>, cx: &mut TestAppContext) -> Option<&'static str> {
    cx.update_window(window.into(), |_, window, _| {
        GATE.into_iter().find(|id| {
            window
                .try_find(*id)
                .is_some_and(|found| found.focused() == Some(true))
        })
    })
    .unwrap()
}

fn press(window: WindowHandle<SubspaceApp>, key: &str, cx: &mut TestAppContext) {
    cx.update_window(window.into(), |_, window, cx| window.press(key, cx))
        .unwrap();
    settle(window, cx);
}

/// Every focus Tab or Shift-Tab reaches from `state`, which must all be
/// the gate's own controls.
fn visits(state: &str, key: &str, cx: &mut TestAppContext) -> Vec<&'static str> {
    let root = tempfile::tempdir().unwrap();
    let (window, _) = gated(state, root.path(), cx);
    let mut seen = Vec::new();
    for _ in 0..6 {
        press(window, key, cx);
        let at = focused(window, cx);
        assert!(at.is_some(), "{state}: {key} left the gate");
        seen.extend(at);
    }
    seen
}

#[gpui_kit::test]
fn tab_and_shift_tab_cycle_the_gate_s_own_controls_in_every_state(cx: &mut TestAppContext) {
    for (state, controls) in [
        (
            "required-signed-out",
            &["sign-in-gate-quit", "sign-in-gate-sign-in"][..],
        ),
        (
            "required-waiting",
            &[
                "sign-in-gate-quit",
                "sign-in-gate-cancel",
                "sign-in-gate-reopen",
            ][..],
        ),
        (
            "required-failed",
            &[
                "sign-in-gate-quit",
                "sign-in-gate-retry",
                "sign-in-gate-sign-in",
            ][..],
        ),
    ] {
        for key in ["tab", "shift-tab"] {
            let seen = visits(state, key, cx);
            for control in controls {
                assert!(
                    seen.contains(control),
                    "{state} {key} never reached {control}"
                );
            }
        }
    }
}

/// The sign-in button disappears once sign-in starts: focus moves to a
/// control the gate still shows, and Tab never reaches the toolbar.
#[gpui_kit::test]
fn focus_stays_in_the_gate_when_its_focused_control_goes(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, _) = gated("required-signed-out", root.path(), cx);
    assert_eq!(focused(window, cx), Some("sign-in-gate-sign-in"));
    cx.update(|cx| crate::account::fixture("required-waiting", cx).unwrap());
    settle(window, cx);
    assert!(focused(window, cx).is_some(), "focus followed the dialog");
    for _ in 0..4 {
        press(window, "tab", cx);
        assert!(
            focused(window, cx).is_some(),
            "Tab reached a control behind"
        );
    }
}

/// With the quit confirmation on, the dialog over the gate takes the
/// keyboard: Tab reaches its buttons and Enter completes.
#[gpui_kit::test]
fn the_quit_confirmation_over_the_gate_answers_the_keyboard(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, _) = gated("required-signed-out", root.path(), cx);
    cx.update(|cx| {
        subspace_dashboard::prefs::set(cx, crate::settings::CONFIRM_QUIT_KEY, "true");
    });
    let mut restart = cx.expect_restart();
    window
        .update(cx, |app, _, cx| app.request_restart(cx))
        .unwrap();
    settle(window, cx);
    let mut reached = false;
    for _ in 0..8 {
        press(window, "tab", cx);
        reached = cx
            .update_window(window.into(), |_, window, _| {
                window.find("confirm-quit").focused() == Some(true)
            })
            .unwrap();
        if reached {
            break;
        }
    }
    assert!(reached, "Tab reached the confirmation's button");
    press(window, "enter", cx);
    assert!(restart.try_recv().unwrap().is_some(), "Enter confirmed it");
}

/// The shell's entry points that toolbar buttons call directly do nothing
/// behind the gate.
#[gpui_kit::test]
fn the_toolbar_s_entry_points_are_inert_behind_the_gate(cx: &mut TestAppContext) {
    use crate::app::ToggleChat;
    use crate::app::terminal_dock::Change;
    let root = tempfile::tempdir().unwrap();
    let (window, _) = gated("required-signed-out", root.path(), cx);
    window
        .update(cx, |app, window, cx| {
            app.open_palette(subspace_dashboard::PaletteMode::Commands, window, cx);
            app.toggle_chat(&ToggleChat, window, cx);
            app.change_terminal(Change::Toggle, window, cx);
            app.switch_to(0, window, cx);
            assert!(app.session.is_none(), "palette");
            assert!(!app.chat_open, "chat");
            assert!(app.shown_terminal().is_none(), "terminal");
        })
        .unwrap();
    settle(window, cx);
    assert_eq!(focused(window, cx), Some("sign-in-gate-sign-in"));
}

/// A notification clicked while sign-in is due waits, and opens once
/// signed in.
#[gpui_kit::test]
fn a_notification_waits_for_sign_in_then_opens(cx: &mut TestAppContext) {
    use crate::notifications::Route;
    let root = tempfile::tempdir().unwrap();
    let (window, fixture) = gated("required-signed-out", root.path(), cx);
    window
        .update(cx, |app, window, cx| {
            app.open_notification(Route::Board("cover-fixture", 1), window, cx)
        })
        .unwrap();
    settle(window, cx);
    assert_eq!(focused(window, cx), Some("sign-in-gate-sign-in"), "held");
    cx.update(|cx| crate::account::fixture("required-signed-in", cx).unwrap());
    settle(window, cx);
    window
        .update(cx, |_, window, cx| {
            assert!(window.try_find("sign-in-gate").is_none());
            let space = fixture.read(cx).focus.clone();
            assert_eq!(window.focused(cx), Some(space), "opened on the Space");
        })
        .unwrap();
    let held = cx.update(|cx| {
        cx.try_global::<crate::app::sign_in_gate::SignInGate>()
            .is_some_and(|state| state.held.is_some())
    });
    assert!(!held, "applied once");
}
