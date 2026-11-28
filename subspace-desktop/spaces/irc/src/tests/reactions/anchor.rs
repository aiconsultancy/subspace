//! The reaction menu opens at the message it is for: the toolbar's React at
//! the hovered message, `r` at the keyboard-focused one.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{SharedString, TestAppContext};
use subspace_framework::dashboard::Dashboard as _;

use super::{open, room, seq};

#[gpui_kit::test]
fn the_toolbar_s_react_opens_at_the_hovered_message_not_the_focused_one(cx: &mut TestAppContext) {
    let (board, hub, cx) = open(cx);
    let conversation = board.read_with(cx, |board, _| board.dock.open[&room()].entity.clone());
    let m1 = seq(&hub, "m1", cx);
    let row = gpui_kit::SharedString::from(format!("irc-line:{m1}:row"));
    let react = gpui_kit::SharedString::from(format!("irc-line:{m1}:react"));
    cx.update(|window, cx| window.hover(row, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let trigger = window.find(react.clone()).bounds();
        conversation.read_with(cx, |view, cx| {
            assert_eq!(view.reacting.anchors.reaction.read(cx).bounds(), trigger);
        });
        window.click(react, cx);
        window.render_frame(cx);
    });
    conversation.read_with(cx, |view, cx| {
        assert!(
            view.reacting.anchors.reaction.read(cx).is_open(),
            "at the hovered message"
        );
        assert!(
            !view.reacting.anchors.keyboard.read(cx).is_open(),
            "not at the focused one's anchor"
        );
    });
    cx.update(|window, cx| {
        board.update(cx, |board, cx| board.deactivate(window, cx));
        assert!(
            !conversation
                .read(cx)
                .reacting
                .anchors
                .reaction
                .read(cx)
                .is_open()
        );
    });
}

#[gpui_kit::test]
fn thank_you_is_an_ordinary_reaction_and_stop_has_its_own_control(cx: &mut TestAppContext) {
    let (_, hub, cx) = open(cx);
    let request = seq(&hub, "r1", cx);
    let row = SharedString::from(format!("irc-line:{request}:row"));
    let react = SharedString::from(format!("irc-line:{request}:react"));
    let stop = SharedString::from(format!("irc-line:{request}:stop"));
    cx.update(|window, cx| window.hover(row.clone(), cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find(stop.clone()).label(), Some("Stop astra"));
        window.click(react, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let mut menu = window.within("popup-menu");
        assert!(menu.try_find_by_label("🛑  Stop sign").is_none());
        assert!(menu.try_find_by_label("✋  Raised hand").is_none());
        menu.click(6usize, cx);
    });
    hub.read_with(cx, |hub, _| {
        assert!(matches!(hub.sent().last(),
            Some(subspace_client_irc::Command::React { msgid, reaction, .. }) if msgid == "r1" && reaction == "🙏"));
    });
    cx.update(|window, cx| window.hover(row, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click(stop, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let mut menu = window.within("popup-menu");
        assert!(menu.try_find_by_label("🙏  Thank you").is_none());
        assert!(menu.try_find_by_label("🛑  Stop sign").is_some());
        assert_eq!(menu.find(1usize).label(), Some("✋  Raised hand"));
        menu.click(1usize, cx);
    });
    hub.read_with(cx, |hub, _| {
        assert!(matches!(hub.sent().last(),
            Some(subspace_client_irc::Command::React { msgid, reaction, .. }) if msgid == "r1" && reaction == "✋"));
    });
}
