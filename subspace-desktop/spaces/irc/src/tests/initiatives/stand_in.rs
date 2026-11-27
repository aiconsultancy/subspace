//! When the pane swaps its toolbar for a stand-in, because Channels has no
//! network any more or the person has lost their last initiative, the
//! toolbar's overflow menus close rather than stay open out of sight, and
//! focus moves to the board rather than staying on a button that went. The
//! menus close when the shell dismisses the Space's menus, too.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_framework::dashboard::Dashboard as _;

use super::closed::memberships;
use super::narrow::narrow;
use super::settle;
use crate::Irc;
use crate::config::Settings;
use crate::hub::Hub;

/// A side of the pane's toolbar, each with its own overflow menu.
#[derive(Clone, Copy, Debug)]
enum Side {
    Leading,
    Trailing,
}

impl Side {
    fn id(self) -> &'static str {
        match self {
            Self::Leading => "workspace-toolbar-leading",
            Self::Trailing => "workspace-toolbar-trailing",
        }
    }

    fn is_open(self, board: &Entity<Irc>, cx: &mut VisualTestContext) -> bool {
        board.read_with(cx, |board, cx| {
            let toolbar = match self {
                Self::Leading => &board.leading,
                Self::Trailing => &board.toolbar,
            };
            toolbar
                .as_ref()
                .is_some_and(|toolbar| toolbar.read(cx).is_open(cx))
        })
    }
}

/// Channels in a narrow pane with `side`'s overflow menu open.
fn with_menu_open(
    side: Side,
    cx: &mut TestAppContext,
) -> (Entity<Irc>, Entity<Hub>, &mut VisualTestContext) {
    let (board, hub, cx) = narrow(cx);
    cx.update(|window, cx| {
        window
            .within(side.id())
            .click("overflow-toolbar-button", cx)
    });
    settle(cx);
    assert!(side.is_open(&board, cx), "{side:?}'s menu opened");
    (board, hub, cx)
}

/// After a stand-in replaces the toolbar: whether the menu is still open,
/// and whether focus is on something drawn, within the board.
struct Outcome {
    open: bool,
    focus_drawn: bool,
}

impl Outcome {
    fn of(side: Side, board: &Entity<Irc>, cx: &mut VisualTestContext) -> Self {
        let focus_drawn = cx.update(|window, cx| board.read(cx).focus.contains_focused(window, cx));
        Self {
            open: side.is_open(board, cx),
            focus_drawn,
        }
    }
}

/// Channels loses every network with `side`'s menu open.
fn lose_every_network(side: Side, cx: &mut TestAppContext) -> Outcome {
    let (board, hub, cx) = with_menu_open(side, cx);
    hub.update(cx, |hub, cx| hub.apply(Settings::default(), cx));
    settle(cx);
    cx.update(|window, _| assert!(window.try_find("irc-unconfigured").is_some()));
    Outcome::of(side, &board, cx)
}

/// The person loses their last initiative with `side`'s menu open.
fn lose_the_last_initiative(side: Side, cx: &mut TestAppContext) -> Outcome {
    let (board, hub, cx) = with_menu_open(side, cx);
    memberships(&hub, Some("fixture-user"), Ok(Vec::new()), cx);
    cx.update(|window, _| assert!(window.try_find("irc-no-initiative").is_some()));
    Outcome::of(side, &board, cx)
}

/// The shell dismisses the Space's menus with `side`'s menu open; whether
/// the menu is still open after.
fn dismiss_the_space(side: Side, cx: &mut TestAppContext) -> bool {
    let (board, _hub, cx) = with_menu_open(side, cx);
    cx.update(|window, cx| board.update(cx, |board, cx| board.dismiss(window, cx)));
    settle(cx);
    side.is_open(&board, cx)
}

#[gpui_kit::test]
fn losing_every_network_closes_the_pickers_menu(cx: &mut TestAppContext) {
    let outcome = lose_every_network(Side::Leading, cx);
    assert!(!outcome.open, "the menu closed");
    assert!(
        outcome.focus_drawn,
        "focus is on the board, not a button that went"
    );
}

#[gpui_kit::test]
fn losing_every_network_closes_the_actions_menu(cx: &mut TestAppContext) {
    let outcome = lose_every_network(Side::Trailing, cx);
    assert!(!outcome.open, "the menu closed");
    assert!(
        outcome.focus_drawn,
        "focus is on the board, not a button that went"
    );
}

#[gpui_kit::test]
fn losing_the_last_initiative_closes_the_pickers_menu(cx: &mut TestAppContext) {
    let outcome = lose_the_last_initiative(Side::Leading, cx);
    assert!(!outcome.open, "the menu closed");
    assert!(
        outcome.focus_drawn,
        "focus is on the board, not a button that went"
    );
}

#[gpui_kit::test]
fn losing_the_last_initiative_closes_the_actions_menu(cx: &mut TestAppContext) {
    let outcome = lose_the_last_initiative(Side::Trailing, cx);
    assert!(!outcome.open, "the menu closed");
    assert!(
        outcome.focus_drawn,
        "focus is on the board, not a button that went"
    );
}

#[gpui_kit::test]
fn dismissing_the_space_closes_the_pickers_menu(cx: &mut TestAppContext) {
    assert!(!dismiss_the_space(Side::Leading, cx), "the menu closed");
}

#[gpui_kit::test]
fn dismissing_the_space_closes_the_actions_menu(cx: &mut TestAppContext) {
    assert!(!dismiss_the_space(Side::Trailing, cx), "the menu closed");
}
