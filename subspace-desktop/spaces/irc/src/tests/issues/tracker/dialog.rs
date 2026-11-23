//! A message action's dialog writes only under the account, organisation
//! and tracker it opened under.

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use subspace_framework::contracts::issues::IssueScope;

use super::{creating, found, open, open_make_issue, said};

#[gpui_kit::test]
fn a_dialog_writes_nothing_once_the_tracker_it_opened_under_changed(cx: &mut TestAppContext) {
    let mock = creating();
    let (conversation, hub, cx) = open(cx, true, Some(&mock));
    open_make_issue(&conversation, &hub, cx);
    // Another organisation's tracker, with the same project keys.
    hub.update(cx, |hub, cx| {
        let other = IssueScope::new("other-org", "Other organisation", "http://127.0.0.1:10")
            .with_project_keys(["NAV"]);
        hub.issues.set_scope(Some(other));
        cx.notify();
    });
    cx.update(|window, cx| window.click("irc-tracker-confirm", cx));
    cx.run_until_parked();
    assert!(found("irc-tracker-dialog".into(), cx), "the dialog stays");
    conversation.read_with(cx, |conversation, _| {
        let dialog = conversation.tracker.dialog.as_ref().expect("open");
        assert_eq!(
            dialog.error.as_deref(),
            Some(crate::tracker::SCOPE_CHANGED),
            "a plain reason"
        );
    });
    assert!(mock.requests().is_empty(), "nothing was sent");
    hub.read_with(cx, |hub, _| assert!(said(hub).is_empty()));
}
