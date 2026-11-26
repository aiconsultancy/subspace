//! A request addressed mid-message, with progress and the answer in its thread.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, VisualTestContext};
use subspace_client_irc::Event;

use super::{asked, inject, progress, reply, settle, ship};

#[gpui_kit::test]
fn inline_request_progress_and_final_answer_refresh_both_views(cx: &mut TestAppContext) {
    let (_, hub, cx) = ship(cx);
    let mut request = asked();
    request.text = "Thanks, @aurora-lyra. Could you check the baseline?".into();
    inject(
        &hub,
        vec![Event::Line(request), Event::Line(progress("tool", 5))],
        cx,
    );
    settle(cx);
    cx.update(|window, cx| {
        assert_eq!(
            window.find("irc-status:ask1").label(),
            Some("aurora-lyra: Using a tool. Opens the thread")
        );
        window.click("irc-status:ask1", cx);
    });
    settle(cx);
    inject(
        &hub,
        vec![Event::Line(reply("aurora-lyra", "result", "ask1", 7))],
        cx,
    );
    assert_answer(cx, "Using a tool");
    inject(&hub, vec![Event::Line(progress("completed", 8))], cx);
    assert_answer(cx, "Done in");
}

fn assert_answer(cx: &mut VisualTestContext, stage: &str) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, _| {
        let status = window.find("irc-status:ask1");
        let label = status.label().unwrap_or_default();
        assert!(
            label.contains(stage) && label.contains("1 reply"),
            "{label}"
        );
        assert!(window.find("inspector-section:Replies · 1").visible());
        let replies = window.within("irc-thread-replies");
        assert!(
            replies
                .try_find_by_label("aurora-lyra: Reply result.")
                .is_some()
        );
    });
}
