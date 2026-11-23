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

#[gpui_kit::test]
fn a_pill_shows_its_value_cleaned_and_takes_back_the_exact_one(cx: &mut TestAppContext) {
    let (_board, hub, cx) = open(cx);
    let odd = "\u{1F44D} from\u{202E} you";
    receive(&hub, vec![reaction("m1", odd, "subspace")], cx);
    let m1 = seq(&hub, "m1", cx);
    let pill = format!("irc-line:{m1}:reaction:0");
    cx.update(|window, _| {
        assert_eq!(
            super::label(window, &pill).as_deref(),
            Some("\u{1F44D}fromyou from you")
        );
    });
    cx.update(|window, cx| window.click(SharedString::from(pill), cx));
    cx.run_until_parked();
    hub.read_with(cx, |hub, _| {
        assert!(matches!(
            hub.sent().last(),
            Some(Command::Unreact { reaction, .. }) if reaction == odd
        ));
    });
}
