//! A message action's dialog writes only under the account, organisation
//! and tracker it opened under.

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use subspace_framework::contracts::issues::IssueScope;

use super::{creating, found, notice, open, open_make_issue, open_make_issue_on, said};

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

#[gpui_kit::test]
fn the_dialog_shows_every_word_it_will_write(cx: &mut TestAppContext) {
    let mock = creating();
    let (conversation, hub, cx) = open(cx, true, Some(&mock));
    let tail = "and later [docs](https://evil.example) @[Ada](member:x)";
    let long = format!("{} {tail}", "Pan stalls while tiles decode.".repeat(12));
    let room = crate::fixtures::channel("#subspace");
    let mut line = crate::fixtures::message(&room, "bob", &long, 5);
    line.id = Some("m3".into());
    crate::tests::inject(&hub, vec![subspace_client_irc::Event::Line(line)], cx);
    open_make_issue_on(&conversation, (&hub, "m3"), cx);
    let shown = cx.update(|window, _| {
        window
            .try_find("irc-tracker-quote-scroll")
            .and_then(|found| found.label().map(str::to_owned))
            .unwrap_or_default()
    });
    assert!(shown.len() > 280 && shown.ends_with(tail), "{shown}");
    assert!(mock.requests().is_empty());
}

#[gpui_kit::test]
fn make_issue_writes_only_from_its_dialog(cx: &mut TestAppContext) {
    let mock = creating();
    let (conversation, hub, cx) = open(cx, true, Some(&mock));
    open_make_issue(&conversation, &hub, cx);
    assert!(
        mock.requests().is_empty(),
        "opening the dialog writes nothing"
    );
    let title = conversation.read_with(cx, |conversation, cx| {
        let dialog = conversation.tracker.dialog.as_ref().expect("open");
        assert_eq!(dialog.chosen.as_deref(), Some("NAV"));
        dialog.title.read(cx).value().to_string()
    });
    assert_eq!(title, "Pan stalls while tiles decode");
    cx.update(|window, cx| window.click("irc-tracker-confirm", cx));
    cx.run_until_parked();
    assert_eq!(notice(cx), "Created NAV-14: Pan stalls while tiles decode.");
    assert!(!found("irc-tracker-dialog".into(), cx));
    let posts: Vec<_> = mock
        .requests()
        .into_iter()
        .filter(|request| request.method == "POST")
        .collect();
    assert_eq!(posts.len(), 1, "one write");
    let body: serde_json::Value = serde_json::from_str(&posts[0].body).unwrap();
    assert_eq!(
        (body["projectId"].as_str(), body["title"].as_str()),
        (Some("project-nav"), Some(title.as_str()))
    );
    assert!(
        body["description"]
            .as_str()
            .unwrap()
            .contains("by bob in #subspace on Acme IRC")
    );
    conversation.read_with(cx, |conversation, _| {
        assert_eq!(conversation.tracker.refreshed, ["NAV-14"])
    });
    hub.read_with(cx, |hub, _| {
        assert!(said(hub).is_empty(), "nothing posted to the channel")
    });
}
