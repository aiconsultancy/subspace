//! The one sending rule on the real transcript: a new ✋ or 🛑 of ours goes
//! only from the labelled **Stop <ship>** item, whatever path asks and
//! whether or not the app recognises the message as an agent's.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, SharedString, TestAppContext, VisualTestContext};
use subspace_client_irc::{Command, Event};

use super::{inject, label, message, open, reaction, receive, room, seq, with_id};
use crate::hub::Hub;

const HAND: &str = "\u{270B}";

fn sent(hub: &Entity<Hub>, cx: &mut VisualTestContext) -> usize {
    hub.read_with(cx, |hub, _| hub.sent().len())
}

fn notice(cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| window.render_frame(cx));
    cx.update(|window, _| label(window, "irc-reaction-notice").unwrap_or_default())
}

/// Clicks `pill` and says whether anything was sent.
fn click_sends(hub: &Entity<Hub>, pill: String, cx: &mut VisualTestContext) -> bool {
    let before = sent(hub, cx);
    cx.update(|window, cx| window.click(SharedString::from(pill), cx));
    cx.run_until_parked();
    sent(hub, cx) != before
}

#[gpui_kit::test]
fn a_stop_in_the_overflow_of_a_running_request_is_never_sent(cx: &mut TestAppContext) {
    let (_board, hub, cx) = open(cx);
    let ten = ["👍", "🎉", "👀", "✅", "😄", "🚀", "🔥", "💯", "🙏", "👏"];
    let mut updates: Vec<_> = ten
        .iter()
        .map(|value| reaction("r1", value, "bob"))
        .collect();
    updates.push(reaction("r1", HAND, "carol"));
    receive(&hub, updates, cx);
    let r1 = seq(&hub, "r1", cx);
    cx.update(|window, _| {
        let more = label(window, &format!("irc-line:{r1}:reactions-more"));
        assert_eq!(more.as_deref(), Some("1 more reactions"), "✋ is behind +1");
    });
    // Open +1 and choose its one item, ✋, from the keyboard.
    let before = sent(&hub, cx);
    cx.update(|window, cx| {
        window.click(
            SharedString::from(format!("irc-line:{r1}:reactions-more")),
            cx,
        )
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    assert_eq!(sent(&hub, cx), before, "nothing sent");
    assert!(
        notice(cx).contains("Stop astra"),
        "the item was chosen and explained"
    );
}

#[gpui_kit::test]
fn a_stop_pill_on_a_request_with_no_status_yet_sends_nothing(cx: &mut TestAppContext) {
    let (_board, hub, cx) = open(cx);
    inject(
        &hub,
        vec![Event::Line(with_id(
            message(&room(), "subspace", "astra: deploy the preview", 8),
            "fresh",
        ))],
        cx,
    );
    receive(&hub, vec![reaction("fresh", HAND, "bob")], cx);
    let fresh = seq(&hub, "fresh", cx);
    assert!(
        !click_sends(&hub, format!("irc-line:{fresh}:reaction:0"), cx),
        "Stargate could stop the queued turn"
    );
    assert!(notice(cx).contains("only from Stop in the message actions"));
}

#[gpui_kit::test]
fn a_stop_pill_on_an_agent_reply_without_an_account_sends_nothing(cx: &mut TestAppContext) {
    let (_board, hub, cx) = open(cx);
    let mut answer = with_id(message(&room(), "astra", "Deploying now.", 9), "a1");
    answer.reply_to = Some("r1".into());
    inject(&hub, vec![Event::Line(answer)], cx);
    receive(&hub, vec![reaction("a1", "\u{1F6D1}\u{FE0F}", "bob")], cx);
    let a1 = seq(&hub, "a1", cx);
    assert!(!click_sends(&hub, format!("irc-line:{a1}:reaction:0"), cx));
}

#[gpui_kit::test]
fn taking_back_our_own_stop_and_an_ordinary_pill_still_send(cx: &mut TestAppContext) {
    let (_board, hub, cx) = open(cx);
    receive(
        &hub,
        vec![
            reaction("m1", HAND, "subspace"),
            reaction("m1", "👍", "bob"),
        ],
        cx,
    );
    let m1 = seq(&hub, "m1", cx);
    assert!(click_sends(&hub, format!("irc-line:{m1}:reaction:0"), cx));
    hub.read_with(cx, |hub, _| {
        assert!(
            matches!(hub.sent().last(), Some(Command::Unreact { reaction, .. }) if reaction == HAND)
        );
    });
    assert!(click_sends(&hub, format!("irc-line:{m1}:reaction:1"), cx));
}

#[gpui_kit::test]
fn a_stop_s_acknowledgement_survives_another_reaction_sent_before_its_echo(
    cx: &mut TestAppContext,
) {
    let (board, hub, cx) = open(cx);
    let conversation = board.read_with(cx, |board, _| board.dock.open[&room()].entity.clone());
    conversation.update(cx, |view, cx| {
        let stop = Some("astra".to_owned());
        crate::conversation::reacting::toggle(view, ("r1".into(), HAND.into()), true, stop, cx);
        crate::conversation::reacting::toggle(view, ("m1".into(), "👍".into()), true, None, cx);
    });
    receive(
        &hub,
        vec![
            reaction("m1", "👍", "subspace"),
            reaction("r1", HAND, "subspace"),
        ],
        cx,
    );
    assert_eq!(
        notice(cx),
        crate::reactions::stop::stop_sent(HAND, "astra"),
        "the stop's notice once its own echo arrives"
    );
}

#[gpui_kit::test]
fn a_stop_item_left_open_is_checked_again_when_chosen(cx: &mut TestAppContext) {
    let (board, hub, cx) = open(cx);
    let conversation = board.read_with(cx, |board, _| board.dock.open[&room()].entity.clone());
    let before = sent(&hub, cx);
    // A stale item naming astra, chosen on a message that is not hers.
    conversation.update(cx, |view, cx| {
        let stop = Some("astra".to_owned());
        crate::conversation::reacting::toggle(view, ("m1".into(), HAND.into()), true, stop, cx);
    });
    assert_eq!(sent(&hub, cx), before, "m1 is no agent's message");
    // The roster loses astra while her request's menu is open: the person
    // now belongs only to an initiative without a fleet.
    let fleetless = serde_json::from_value(serde_json::json!([
        {"id": "initiative-local", "name": "Local", "channels": "#subspace"}
    ]))
    .unwrap();
    hub.update(cx, |hub, cx| {
        hub.initiatives.catalog.clone().update(cx, |catalog, cx| {
            catalog.set_memberships_for_test(Some("fixture-user".into()), Ok(fleetless), cx)
        })
    });
    cx.run_until_parked();
    // The new initiative's channels take over the autojoin.
    let before = sent(&hub, cx);
    conversation.update(cx, |view, cx| {
        let stop = Some("astra".to_owned());
        crate::conversation::reacting::toggle(view, ("r1".into(), HAND.into()), true, stop, cx);
    });
    assert_eq!(sent(&hub, cx), before, "astra is no longer a roster agent");
    assert!(
        notice(cx).contains("only from Stop in the message actions"),
        "explained, not sent"
    );
}

#[gpui_kit::test]
fn a_stop_is_acknowledged_by_its_echo_when_the_message_shows_every_reaction_it_can(
    cx: &mut TestAppContext,
) {
    let (board, hub, cx) = open(cx);
    // Twenty other reactions already fill r1's display.
    let twenty: Vec<_> = (0x1F600..0x1F614)
        .filter_map(char::from_u32)
        .map(|face| reaction("r1", &face.to_string(), "bob"))
        .collect();
    receive(&hub, twenty, cx);
    let conversation = board.read_with(cx, |board, _| board.dock.open[&room()].entity.clone());
    conversation.update(cx, |view, cx| {
        let stop = Some("astra".to_owned());
        crate::conversation::reacting::toggle(view, ("r1".into(), HAND.into()), true, stop, cx);
    });
    receive(&hub, vec![reaction("r1", HAND, "subspace")], cx);
    assert_eq!(
        notice(cx),
        crate::reactions::stop::stop_sent(HAND, "astra"),
        "the echo arrived though the display had no room for it"
    );
}

#[gpui_kit::test]
fn a_stop_typed_by_hand_in_a_channel_is_refused_and_points_to_its_item(cx: &mut TestAppContext) {
    let (board, hub, cx) = open(cx);
    let conversation = board.read_with(cx, |board, _| board.dock.open[&room()].entity.clone());
    let input = conversation.read_with(cx, |view, _| view.composer().clone());
    let before = sent(&hub, cx);
    let stop = "/raw @+draft/react=\u{270B};+draft/reply=r1 TAGMSG #subspace";
    crate::tests::completion::submit(&input, stop, cx);
    assert_eq!(sent(&hub, cx), before, "nothing sent");
    let error = input.read_with(cx, |input, _| input.error().map(str::to_owned));
    assert!(
        error
            .as_deref()
            .unwrap_or_default()
            .contains("only from Stop in the message actions"),
        "{error:?}"
    );
    let thumbs = "/raw @+draft/react=\u{1F44D};+draft/reply=m1 TAGMSG #subspace";
    crate::tests::completion::submit(&input, thumbs, cx);
    assert_eq!(
        sent(&hub, cx),
        before + 1,
        "an ordinary reaction goes as the gate allows"
    );
}
