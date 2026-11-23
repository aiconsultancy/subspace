//! A write goes only to the tracker the Issues scope names: with the scope
//! naming one tracker and Channels' client another, both having a `NAV`
//! project, neither a dialog nor a typed command sends anything.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_framework::contracts::issues::IssueScope;

use super::{creating, found, notice, open, open_make_issue, said, submit, text};
use crate::hub::Hub;

/// The scope now names another tracker with the same project key.
fn elsewhere(hub: &Entity<Hub>, cx: &mut VisualTestContext) {
    hub.update(cx, |hub, cx| {
        let other = IssueScope::new("subspace-api", "Subspace API", "http://127.0.0.1:10/api")
            .with_project_keys(["NAV"]);
        hub.issues.set_scope(Some(other));
        cx.notify();
    });
}

#[gpui_kit::test]
fn make_issue_writes_nothing_to_another_tracker(cx: &mut TestAppContext) {
    let mock = creating();
    let (conversation, hub, cx) = open(cx, true, Some(&mock));
    elsewhere(&hub, cx);
    open_make_issue(&conversation, &hub, cx);
    cx.update(|window, cx| window.click("irc-tracker-confirm", cx));
    cx.run_until_parked();
    assert!(found("irc-tracker-dialog".into(), cx), "the dialog stays");
    assert_eq!(notice(cx), crate::tracker::WRONG_TRACKER);
    assert!(mock.requests().is_empty(), "nothing was sent");
}

#[gpui_kit::test]
fn typed_commands_write_nothing_to_another_tracker(cx: &mut TestAppContext) {
    let mock = creating();
    let (conversation, hub, cx) = open(cx, true, Some(&mock));
    let input = super::composer(&conversation, cx);
    elsewhere(&hub, cx);
    for command in ["/assign NAV-9 to me", "/decision NAV-9 Decode on a pool"] {
        submit(&input, command, cx);
        assert_eq!(notice(cx), crate::tracker::WRONG_TRACKER, "{command}");
        assert_eq!(text(&input, cx), command, "the draft stays");
    }
    assert!(mock.requests().is_empty(), "nothing was sent");
    hub.read_with(cx, |hub, _| assert!(said(hub).is_empty()));
}
