//! The conversation menu the toolbar leads with while the navigator is
//! hidden keeps every conversation reachable however many there are: its
//! height is bounded by the window and it scrolls.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{ScrollDelta, TestAppContext, point, px, size};
use subspace_client_irc::{ConnectionStatus, Event};

use super::{inject, mount, settings};
use crate::dock;
use crate::fixtures::{channel, network_id};

/// More conversations than a window this tall shows at once.
const ROOMS: usize = 40;
const WINDOW_HEIGHT_PX: f32 = 480.;

#[gpui_kit::test]
fn a_long_conversation_menu_scrolls_within_the_window(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    let connected = Event::Status {
        network: network_id(),
        status: ConnectionStatus::Connected {
            nick: "subspace".into(),
        },
    };
    let joined = (0..ROOMS).map(|index| Event::Joined {
        buffer: channel(&format!("#room-{index:02}")),
    });
    inject(&hub, std::iter::once(connected).chain(joined).collect(), cx);
    board.update(cx, dock::toggle_buffers);
    cx.simulate_resize(size(px(1280.), px(WINDOW_HEIGHT_PX)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("irc-conversation-menu", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let menu = window.find("popup-menu").bounds();
        assert!(
            menu.bottom() <= px(WINDOW_HEIGHT_PX),
            "the menu {menu:?} ends inside the window"
        );
        let entries = window.within("popup-menu");
        let (index, last) = (0usize..64)
            .rev()
            .find_map(|index| {
                let label = entries.try_find(index)?.label()?.to_owned();
                Some((index, label))
            })
            .unwrap_or_default();
        assert_eq!(last, "#room-39", "the last is in the menu");
        // Scrolled to its end, the last conversation can be chosen: a menu
        // that only clipped its rows would leave it out of sight.
        let down = ScrollDelta::Pixels(point(px(0.), px(-10_000.)));
        window.scroll("popup-menu", down, cx);
        window.within("popup-menu").click(index, cx);
    });
    cx.run_until_parked();
    board.read_with(cx, |board, _| {
        assert_eq!(
            board.dock.focused,
            Some(channel("#room-39")),
            "choosing it opened that conversation"
        );
    });
}
