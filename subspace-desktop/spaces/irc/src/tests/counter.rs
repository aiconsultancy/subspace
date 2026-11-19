//! Counter limits track both the draft kind and live capability changes.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_client_irc::Event;
use subspace_framework::dashboard::ChatInput;

use super::completion::composer;
use super::inject;
use crate::fixtures::network_id;

fn replace(input: &Entity<ChatInput>, text: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_range(0..input.text(cx).len(), text, window, cx);
        })
    });
    cx.run_until_parked();
}

fn status(cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .find("chat-input-counter")
            .label()
            .unwrap()
            .to_owned()
    })
}

#[gpui_kit::test]
fn the_counter_uses_command_limits_and_reacts_to_capability_changes(cx: &mut TestAppContext) {
    let (conversation, input, cx) = composer(cx);
    let hub = conversation.read_with(cx, |view, _| view.hub.clone());
    inject(
        &hub,
        vec![Event::Capabilities {
            network: network_id(),
            enabled: vec!["draft/multiline".into()],
        }],
        cx,
    );
    let twelve = ["line"; 12].join("\n");
    replace(&input, &twelve, cx);
    assert!(status(cx).contains("12 / 100 lines"));
    replace(&input, &format!("/quote {twelve}"), cx);
    let command = status(cx);
    assert!(command.contains("12 / 10 lines"));
    assert!(command.contains("Over the limit"));
    cx.update(|window, _| assert!(window.try_find("irc-save-draft").is_some()));
    replace(&input, &format!("//{twelve}"), cx);
    assert!(status(cx).contains("12 / 100 lines"));
    inject(
        &hub,
        vec![Event::Capabilities {
            network: network_id(),
            enabled: vec![],
        }],
        cx,
    );
    let unbatched = status(cx);
    assert!(unbatched.contains("12 / 10 lines"));
    assert!(unbatched.contains("Over the limit"));
    cx.update(|window, _| assert!(window.try_find("irc-save-draft").is_some()));
}
