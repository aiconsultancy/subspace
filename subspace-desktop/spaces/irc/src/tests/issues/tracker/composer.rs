//! The composer's keys and modes around a tracker command: Tab only
//! completes, and reply mode keeps a command and never sends it.

use gpui_kit::TestAppContext;

use super::{composer, labels, open, said, signed_in, submit, text};

#[gpui_kit::test]
fn tab_on_a_whole_command_only_completes_and_writes_nothing(cx: &mut TestAppContext) {
    let mock = signed_in();
    let (conversation, hub, cx) = open(cx, true, Some(&mock));
    let input = composer(&conversation, cx);
    for typed in ["/ready NAV-9", " /assign NAV-9 to me"] {
        cx.update(|window, cx| input.update(cx, |input, cx| input.set_text("", window, cx)));
        cx.simulate_input(typed.trim_start());
        assert_eq!(
            labels(&conversation, cx).first().map(String::as_str),
            Some(typed.trim_start().split(' ').next().unwrap_or_default()),
            "the run row is highlighted first"
        );
        cx.simulate_keystrokes("tab");
        cx.run_until_parked();
        assert!(
            text(&input, cx).starts_with(typed.trim_start()),
            "the draft stays: {}",
            text(&input, cx)
        );
    }
    assert!(mock.requests().is_empty(), "Tab never runs a command");
    conversation.read_with(cx, |conversation, _| {
        assert!(conversation.tracker.refreshed.is_empty())
    });
    hub.read_with(cx, |hub, _| {
        assert!(said(hub).is_empty(), "{:?}", said(hub))
    });
}

#[gpui_kit::test]
fn a_tracker_command_in_reply_mode_is_kept_and_never_reaches_irc(cx: &mut TestAppContext) {
    let mock = signed_in();
    let (conversation, hub, cx) = open(cx, true, Some(&mock));
    let tags = subspace_client_irc::Event::Capabilities {
        network: crate::fixtures::network_id(),
        enabled: vec!["message-tags".to_owned()],
    };
    crate::tests::inject(&hub, vec![tags], cx);
    let input = conversation.update_in(cx, |view, window, cx| {
        // This fixture's root message is held in the room.
        let draft = crate::compose::reply::select(view, ("m1", 1), window, cx);
        draft.read(cx).input().clone()
    });
    for command in [
        "/decision NAV-2 Decode on a pool",
        "/assign NAV-2 to me",
        "/ready NAV-2",
    ] {
        submit(&input, command, cx);
        assert_eq!(text(&input, cx), command, "the draft stays");
        assert_eq!(
            input.read_with(cx, |input, _| input.error().map(str::to_owned)),
            Some(crate::compose::reply::COMMAND.to_owned()),
            "{command}: kept with an explanation"
        );
    }
    assert!(mock.requests().is_empty(), "nothing reached the tracker");
    hub.read_with(cx, |hub, _| {
        assert!(said(hub).is_empty(), "{:?}", said(hub))
    });
}

#[gpui_kit::test]
fn a_run_row_runs_only_the_plan_it_showed(cx: &mut TestAppContext) {
    use subspace_framework::contracts::issues::IssueScope;
    let mock = signed_in();
    let (conversation, hub, cx) = open(cx, true, Some(&mock));
    let input = composer(&conversation, cx);
    cx.simulate_input("/assign NAV-9 to me");
    assert_eq!(
        labels(&conversation, cx).first().map(String::as_str),
        Some("/assign")
    );
    // The tracker changes under the row before Return.
    hub.update(cx, |hub, cx| {
        let other = IssueScope::new("subspace-api", "Subspace API", "http://127.0.0.1:10/api")
            .with_project_keys(["NAV"]);
        hub.issues.set_scope(Some(other));
        cx.notify();
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(text(&input, cx), "/assign NAV-9 to me", "the draft stays");
    assert_eq!(
        input.read_with(cx, |input, _| input.error().map(str::to_owned)),
        Some(crate::compose::RUN_CHANGED.to_owned())
    );
    assert_eq!(
        labels(&conversation, cx).first().map(String::as_str),
        Some("/assign"),
        "the new row shows for a fresh Return"
    );
    assert!(mock.requests().is_empty(), "nothing was written");
    hub.read_with(cx, |hub, _| assert!(said(hub).is_empty()));
}

#[gpui_kit::test]
fn a_command_waits_with_its_draft_while_another_write_runs(cx: &mut TestAppContext) {
    let mock = signed_in();
    let (conversation, _hub, cx) = open(cx, true, Some(&mock));
    let input = composer(&conversation, cx);
    conversation.update(cx, |view, _| view.tracker.busy = true);
    submit(&input, "/ready NAV-9", cx);
    assert_eq!(text(&input, cx), "/ready NAV-9", "the draft stays");
    assert_eq!(
        input.read_with(cx, |input, _| input.error().map(str::to_owned)),
        Some(crate::tracker::BUSY.to_owned())
    );
    assert!(mock.requests().is_empty());
}

#[gpui_kit::test]
fn a_run_row_runs_only_with_the_client_it_was_shown_under(cx: &mut TestAppContext) {
    let mock = signed_in();
    let other = signed_in();
    let (conversation, hub, cx) = open(cx, true, Some(&mock));
    let input = composer(&conversation, cx);
    cx.simulate_input("/ready NAV-9");
    assert_eq!(
        labels(&conversation, cx).first().map(String::as_str),
        Some("/ready")
    );
    // Another sign-in replaces Channels' client before Return.
    hub.update(cx, |hub, _| {
        let url = format!("{}/api", other.base_url());
        let client = subspace_api_client::Client::loopback(&url, None)
            .unwrap()
            .without_live();
        hub.tracker.test_connect(client);
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(text(&input, cx), "/ready NAV-9", "the draft stays");
    assert_eq!(
        input.read_with(cx, |input, _| input.error().map(str::to_owned)),
        Some(crate::compose::RUN_CHANGED.to_owned())
    );
    assert!(mock.requests().is_empty() && other.requests().is_empty());
    hub.read_with(cx, |hub, _| assert!(said(hub).is_empty()));
}
