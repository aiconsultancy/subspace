//! When the API cannot be reached at all, Channels says so in the words
//! every Space uses, from the memberships read or the account's login
//! requests, and offers **Reload** when it has nothing else to show. A
//! refusal or another failure is not taken for it.

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use subspace_api_client::{Error, unreachable_message};

use super::super::{channel_events, inject, mount, settings};
use super::closed::{memberships, running, status};
use super::settle;
use crate::config::Settings;

#[gpui_kit::test]
fn an_unreachable_api_is_named_while_the_configured_networks_run(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, channel_events(), cx);
    memberships(&hub, Some("fixture-user"), Err(Error::Network), cx);
    assert_eq!(running(&hub, cx), Some(1));
    assert_eq!(
        status(&board, cx),
        Some(format!(
            "IRC · 1 network · 1 connected · {}",
            unreachable_message()
        ))
    );
}

#[gpui_kit::test]
fn with_no_network_to_show_the_pane_says_why_and_offers_reload(cx: &mut TestAppContext) {
    let (_board, hub, cx) = mount(cx, Settings::default());
    memberships(&hub, Some("fixture-user"), Err(Error::Network), cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find("irc-unreachable").visible());
        assert!(window.find("irc-unreachable-reload").visible());
        assert!(
            window.try_find("irc-unconfigured").is_none(),
            "not No IRC networks"
        );
    });
}

#[gpui_kit::test]
fn a_refusal_or_another_failure_is_not_an_unreachable_api(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, channel_events(), cx);
    memberships(&hub, None, Err(Error::UnexpectedStatus(500)), cx);
    assert_eq!(
        status(&board, cx).as_deref(),
        Some("IRC · 1 network · 1 connected · Initiatives unavailable. Reload to retry.")
    );
    memberships(&hub, None, Err(Error::Forbidden), cx);
    assert_eq!(
        status(&board, cx).as_deref(),
        Some("IRC · 1 network · 1 connected")
    );
}

#[gpui_kit::test]
fn an_unreachable_api_is_named_once_from_either_side(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, channel_events(), cx);
    // The account side's request and the memberships read both could not
    // reach the API: the status bar names it once. The account side's
    // retries are covered with its schedule.
    hub.update(cx, |hub, _| assert!(hub.managed.note_reach(true)));
    memberships(&hub, Some("fixture-user"), Err(Error::Network), cx);
    settle(cx);
    let status = status(&board, cx).unwrap_or_default();
    assert_eq!(
        status.matches(&unreachable_message()).count(),
        1,
        "{status}"
    );
    // The memberships read alone names it too.
    hub.update(cx, |hub, _| assert!(hub.managed.note_reach(false)));
    assert_eq!(
        super::closed::status(&board, cx),
        Some(format!(
            "IRC · 1 network · 1 connected · {}",
            unreachable_message()
        ))
    );
    // Both reach it again, or are refused: no longer named.
    memberships(&hub, Some("fixture-user"), Err(Error::Forbidden), cx);
    assert_eq!(
        super::closed::status(&board, cx).as_deref(),
        Some("IRC · 1 network · 1 connected")
    );
}
