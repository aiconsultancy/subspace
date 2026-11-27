//! A narrow pane keeps each of the toolbar's pickers and actions in the
//! strip or in its side's overflow menu.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Bounds, Entity, Pixels, TestAppContext, VisualTestContext, Window, px, size};
use subspace_client_irc::{ConnectionStatus, Event};
use subspace_framework::dashboard::icon_button::IconButtonSize;
use subspace_framework::dashboard::workspace::PANE_MIN_WIDTH_PX;

use super::super::{inject, mount, settings};
use super::{catalogue, member_of_the_catalogue, settle};
use crate::Irc;
use crate::fixtures::network_id;
use crate::hub::Hub;

/// Narrow enough that the pane beside the inspector keeps only its least
/// width (the inspector's least is 260 px).
const WINDOW_WIDTH_PX: f32 = 580.;
/// The initiative picker's entry in the overflow menu.
const INITIATIVE_ENTRY: &str = "Initiative: Design Studio…";

/// Whether `part` lies wholly within `area`.
fn within(part: Bounds<Pixels>, area: Bounds<Pixels>) -> bool {
    part.left() >= area.left()
        && part.right() <= area.right()
        && part.top() >= area.top()
        && part.bottom() <= area.bottom()
}

/// The labels of the open popup menu's entries.
fn menu_labels(window: &mut Window) -> Vec<String> {
    let menu = window.within("popup-menu");
    (0usize..32)
        .filter_map(|index| menu.try_find(index))
        .filter_map(|item| item.label().map(|label| label.to_string()))
        .collect()
}

/// Channels with the navigator hidden, the inspector open and the largest
/// icon buttons, in a window that leaves the pane its least width.
fn narrow(cx: &mut TestAppContext) -> (Entity<Irc>, Entity<Hub>, &mut VisualTestContext) {
    let (board, hub, cx) = mount(cx, settings());
    member_of_the_catalogue(&hub, cx);
    let connected = Event::Status {
        network: network_id(),
        status: ConnectionStatus::Connected {
            nick: "tester".into(),
        },
    };
    inject(&hub, vec![connected], cx);
    cx.update(|_, cx| IconButtonSize::ExtraLarge.apply(cx));
    board.update(cx, crate::dock::toggle_buffers);
    cx.simulate_resize(size(px(WINDOW_WIDTH_PX), px(600.)));
    settle(cx);
    settle(cx);
    (board, hub, cx)
}

/// Checks that each of `controls`, an id and its overflow menu entry, is
/// wholly inside the toolbar's `side` or listed in that side's overflow
/// menu, and returns how many are in the menu.
fn reachable(
    side: &'static str,
    controls: &[(&'static str, String)],
    cx: &mut VisualTestContext,
) -> usize {
    cx.update(|window, cx| {
        let area = window.find(side).bounds();
        let mut hidden = Vec::new();
        for (id, entry) in controls {
            match window.try_find(*id) {
                Some(control) => {
                    let bounds = control.bounds();
                    assert!(control.visible(), "{id}");
                    assert!(within(bounds, area), "{id} {bounds:?} in {side} {area:?}");
                }
                None => hidden.push(entry),
            }
        }
        if !hidden.is_empty() {
            let chevron = window.within(side).find("overflow-toolbar-button").bounds();
            assert!(
                within(chevron, area),
                "{side}'s menu button {chevron:?} in {area:?}"
            );
            window.within(side).click("overflow-toolbar-button", cx);
            let labels = menu_labels(window);
            for entry in &hidden {
                assert!(labels.contains(entry), "{entry} in {side}'s {labels:?}");
            }
            window.within(side).click("overflow-toolbar-button", cx);
        }
        hidden.len()
    })
}

#[gpui_kit::test]
fn every_toolbar_control_stays_reachable_in_a_narrow_pane_at_the_largest_size(
    cx: &mut TestAppContext,
) {
    let (board, _hub, cx) = narrow(cx);
    let network = board.read_with(cx, |board, _| board.list.groups()[0].name.to_string());
    cx.update(|window, _| {
        assert!(window.try_find("workspace-navigator").is_none());
        assert!(window.try_find("workspace-inspector-header").is_some());
        let pane = window.find("workspace-pane").bounds();
        assert!(pane.size.width <= px(PANE_MIN_WIDTH_PX + 1.), "{pane:?}");
    });
    let pickers = [
        ("irc-conversation-menu", network),
        ("irc-initiative-button", INITIATIVE_ENTRY.to_owned()),
    ];
    assert!(
        reachable("workspace-toolbar-leading", &pickers, cx) > 0,
        "the narrow pane sends a picker to the menu"
    );
    let actions = [
        ("irc-find", "Find in conversation".to_owned()),
        ("irc-more", "Join…".to_owned()),
        ("irc-refresh", "Refresh".to_owned()),
    ];
    assert!(
        reachable("workspace-toolbar-trailing", &actions, cx) > 0,
        "the actions yield the line to the pickers' menu button"
    );
}

#[gpui_kit::test]
fn a_chooser_opened_from_the_overflow_menu_closes_when_the_catalogue_changes(
    cx: &mut TestAppContext,
) {
    let (board, hub, cx) = narrow(cx);
    cx.update(|window, cx| {
        assert!(
            window.try_find("irc-initiative-button").is_none(),
            "in the menu"
        );
        window
            .within("workspace-toolbar-leading")
            .click("overflow-toolbar-button", cx);
        let index = menu_labels(window)
            .iter()
            .position(|label| label == INITIATIVE_ENTRY)
            .unwrap_or_else(|| panic!("{INITIATIVE_ENTRY} is in the menu"));
        window.within("popup-menu").click(index, cx);
    });
    settle(cx);
    board.read_with(cx, |board, cx| {
        assert!(board.initiative_menu.is_open());
        assert!(
            matches!(
                subspace_framework::dashboard::Dashboard::modal(board, cx),
                Some(subspace_framework::dashboard::Modal::Menu)
            ),
            "the Space holds its keys while the chooser is open"
        );
        assert!(
            !subspace_framework::dashboard::list::ListDashboard::can_navigate(board, cx),
            "the list does not move behind the chooser"
        );
    });
    let mut changed = catalogue();
    changed.truncate(1);
    hub.update(cx, |hub, cx| {
        hub.initiatives.catalog.clone().update(cx, |catalog, cx| {
            catalog.set_memberships_for_test(Some("fixture-user".into()), Ok(changed), cx)
        })
    });
    settle(cx);
    board.read_with(cx, |board, _| {
        assert!(
            !board.initiative_menu.is_open(),
            "no chooser left open with the old initiatives"
        );
    });
}
