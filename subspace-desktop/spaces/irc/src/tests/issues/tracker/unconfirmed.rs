//! A write the tracker never confirmed may have landed: nothing sends the
//! same thing again from the dialog or the draft it came from.

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use subspace_framework::client_test_support::Mock;

use super::{composer, me, open, open_make_issue, projects, signed_in, submit, text};

#[gpui_kit::test]
fn an_unconfirmed_create_keeps_the_dialog_s_button_off(cx: &mut TestAppContext) {
    let mock = Mock::new(
        |request| match (request.method.as_str(), request.path.as_str()) {
            (_, "/api/v1/me") => (200, me()),
            ("GET", path) if path.starts_with("/api/v1/projects") => (200, projects()),
            ("POST", "/api/v1/issues") => (502, "{}".into()),
            _ => (404, "{}".into()),
        },
    );
    let (conversation, hub, cx) = open(cx, true, Some(&mock));
    open_make_issue(&conversation, &hub, cx);
    for _ in 0..2 {
        cx.update(|window, cx| window.click("irc-tracker-confirm", cx));
        cx.run_until_parked();
    }
    let posts = mock
        .requests()
        .iter()
        .filter(|request| request.method == "POST")
        .count();
    assert_eq!(posts, 1, "sent once, never again from this dialog");
    conversation.read_with(cx, |view, _| {
        let dialog = view.tracker.dialog.as_ref().expect("open");
        assert!(dialog.unconfirmed);
        assert!(
            dialog
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("Check the issue"),
            "says it may have been saved"
        );
    });
}

#[gpui_kit::test]
fn an_unconfirmed_command_is_not_sent_again_as_it_stands(cx: &mut TestAppContext) {
    let mock = signed_in();
    let (conversation, _hub, cx) = open(cx, true, Some(&mock));
    let input = composer(&conversation, cx);
    conversation.update(cx, |view, _| {
        view.tracker.unconfirmed = Some("/decision NAV-9 Decode on a pool".into());
    });
    submit(&input, "/decision NAV-9 Decode on a pool", cx);
    assert!(mock.requests().is_empty(), "nothing is sent again");
    assert_eq!(
        input.read_with(cx, |input, _| input.error().map(str::to_owned)),
        Some(crate::tracker::UNCONFIRMED_AGAIN.to_owned())
    );
    assert_eq!(text(&input, cx), "/decision NAV-9 Decode on a pool");
}
