//! Taking ours back removes the value we reacted with, exactly as the
//! client holds it, not the menu's spelling of it.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{SharedString, TestAppContext};
use subspace_client_irc::Command;

use super::{open, reaction, receive, room, seq};

#[gpui_kit::test]
fn taking_back_a_heart_removes_the_one_we_made(cx: &mut TestAppContext) {
    let (_board, hub, cx) = open(cx);
    // Ours is the bare heart, without the variation selector the menu's
    // Heart carries.
    receive(&hub, vec![reaction("m1", "\u{2764}", "subspace")], cx);
    let m1 = seq(&hub, "m1", cx);
    cx.update(|window, cx| {
        window.click(
            SharedString::from(format!("irc-line:{m1}:add-reaction")),
            cx,
        )
    });
    cx.run_until_parked();
    // Heart is the menu's second item, ticked as ours.
    cx.simulate_keystrokes("down down enter");
    cx.run_until_parked();
    hub.read_with(cx, |hub, _| {
        assert_eq!(
            hub.sent().last(),
            Some(&Command::Unreact {
                buffer: room(),
                msgid: "m1".into(),
                reaction: "\u{2764}".into(),
            })
        );
    });
}
