//! For someone who belongs to no initiative, Channels closes and says why
//! until an administrator adds them to one: no network runs, nothing is
//! sent, and no tool, command or context reaches the old conversations. A
//! read that fails is not taken for no memberships.

use std::sync::mpsc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_api_client::Error;
use subspace_api_client::initiatives::NO_MEMBERSHIPS;
use subspace_api_client::model::MemberInitiative;
use subspace_client_irc::Command;
use subspace_framework::dashboard::Dashboard as _;

use super::super::{channel_events, inject, mount, settings};
use super::{member_of_the_catalogue, settle};
use crate::Irc;
use crate::fixtures::network_id;
use crate::hub::Hub;

/// The networks the runtime was last told to run, by count.
pub(super) fn running(hub: &Entity<Hub>, cx: &mut VisualTestContext) -> Option<usize> {
    hub.read_with(cx, |hub, _| {
        hub.sent().iter().rev().find_map(|command| match command {
            Command::Configure(networks) => Some(networks.len()),
            _ => None,
        })
    })
}

pub(super) fn memberships(
    hub: &Entity<Hub>,
    actor: Option<&str>,
    result: Result<Vec<MemberInitiative>, Error>,
    cx: &mut VisualTestContext,
) {
    hub.update(cx, |hub, cx| {
        hub.initiatives.catalog.clone().update(cx, |catalog, cx| {
            catalog.set_memberships_for_test(actor.map(str::to_owned), result, cx)
        })
    });
    settle(cx);
}

pub(super) fn status(board: &Entity<Irc>, cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| board.read(cx).status(cx).map(|status| status.to_string()))
}

#[gpui_kit::test]
fn a_read_that_fails_is_not_taken_for_no_memberships(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, channel_events(), cx);
    assert_eq!(running(&hub, cx), Some(1), "the configured network runs");
    memberships(
        &hub,
        Some("fixture-user"),
        Err(Error::UnexpectedStatus(503)),
        cx,
    );
    assert_eq!(running(&hub, cx), Some(1), "Channels stays on");
    hub.read_with(cx, |hub, _| assert!(!hub.initiatives.closed()));
    assert_eq!(
        status(&board, cx).as_deref(),
        Some("IRC · 1 network · 1 connected · Initiatives unavailable. Reload to retry.")
    );
}

#[gpui_kit::test]
fn channels_closes_for_someone_in_no_initiative_until_they_join_one(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, channel_events(), cx);
    // The API says the person belongs to no initiative: no network runs,
    // nothing is sent, and the Space shows only why.
    memberships(&hub, Some("fixture-user"), Ok(Vec::new()), cx);
    assert_eq!(running(&hub, cx), Some(0));
    let sent = hub.read_with(cx, |hub, _| hub.sent().len());
    hub.update(cx, |hub, _| hub.send(Command::Connect(network_id())));
    hub.read_with(cx, |hub, _| {
        assert!(hub.initiatives.closed());
        assert_eq!(hub.sent().len(), sent, "nothing connects");
    });
    assert_closed(&board, cx);

    // An administrator adds them: Channels reopens on that initiative, and
    // tells the runtime where each conversation left off.
    let before = hub.read_with(cx, |hub, _| hub.sent().len());
    member_of_the_catalogue(&hub, cx);
    settle(cx);
    assert_eq!(running(&hub, cx), Some(1));
    hub.read_with(cx, |hub, _| {
        assert!(!hub.initiatives.closed());
        assert!(
            hub.sent()[before..]
                .iter()
                .any(|command| matches!(command, Command::Anchors { .. })),
            "catch-up starts where the conversations left off"
        );
        assert_eq!(
            hub.sent().last(),
            Some(&Command::SetAutojoin {
                network: network_id(),
                channels: Some(vec!["#design".into(), "#releases".into()]),
            })
        );
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("irc-no-initiative").is_none());
        assert_eq!(
            window.find("irc-initiative-button").label(),
            Some("Initiative: Design Studio")
        );
    });
}

/// The closed Space: its reason in the pane, no conversation, composer,
/// toolbar or docks, and only the reason after the service in the status
/// bar.
fn assert_closed(board: &Entity<Irc>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find("irc-no-initiative").visible());
        for id in ["dock-area", "irc-toolbar", "irc-initiative-button"] {
            assert!(
                window.try_find(id).is_none(),
                "{id}: no conversation or composer"
            );
        }
    });
    assert_eq!(
        status(board, cx).as_deref(),
        Some(format!("IRC · {NO_MEMBERSHIPS}").as_str())
    );
    cx.update(|_, cx| {
        let board = board.read(cx);
        assert_eq!(board.badge(cx), None);
        assert!(board.docks(cx) == subspace_framework::dashboard::workspace::SpaceDocks::NONE);
    });
}

#[gpui_kit::test]
fn only_an_api_with_initiative_channels_closes_channels(cx: &mut TestAppContext) {
    let (_board, hub, cx) = mount(cx, settings());
    memberships(&hub, Some("fixture-user"), Ok(Vec::new()), cx);
    hub.read_with(cx, |hub, cx| {
        let catalog = hub.initiatives.catalog.read(cx);
        let subspace =
            subspace_api_client::Capabilities::of(subspace_api_client::Backend::Subspace);
        assert!(subspace_api_client::initiatives::not_a_member(
            subspace, catalog
        ));
        // A build without an API, or a NETT organisation whose initiatives
        // carry no channels, keeps its configured networks running.
        assert!(!subspace_api_client::initiatives::not_a_member(
            subspace_api_client::Capabilities::NONE,
            catalog
        ));
    });
}

#[gpui_kit::test]
fn a_read_that_fails_after_none_were_found_keeps_channels_closed(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, channel_events(), cx);
    memberships(&hub, Some("fixture-user"), Ok(Vec::new()), cx);
    let sent = hub.read_with(cx, |hub, _| hub.sent().len());
    // An API blip: nothing connects only to stop again, and the status bar
    // says the API cannot be reached.
    memberships(&hub, None, Err(Error::Network), cx);
    hub.read_with(cx, |hub, _| {
        assert!(hub.initiatives.closed());
        assert_eq!(hub.sent().len(), sent);
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find("irc-no-initiative").visible());
    });
    assert_eq!(
        status(&board, cx),
        Some(format!(
            "IRC · {}",
            subspace_api_client::unreachable_message()
        ))
    );
}

#[gpui_kit::test]
fn nothing_reaches_the_old_conversations_while_channels_is_closed(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, channel_events(), cx);
    board.update(cx, |board, cx| {
        crate::view::reveal(board, &crate::fixtures::channel("#subspace"), cx);
    });
    settle(cx);
    memberships(&hub, Some("fixture-user"), Ok(Vec::new()), cx);
    cx.update(|_, cx| {
        let board = board.read(cx);
        assert!(board.agent_context(cx).is_none());
        assert!(board.subject(cx).is_none());
        assert!(board.search(cx).is_none());
    });
    let commands = board.update(cx, |board, cx| board.commands(cx));
    assert!(commands.is_empty(), "no Join, Connect or Switch initiative");

    // Agent tools are refused, and say why.
    let (requests, received) = mpsc::channel();
    hub.update(cx, |hub, _| hub.take_tools(received));
    let (reply, answer) = mpsc::channel();
    let networks = serde_json::from_value(serde_json::json!({"action": "networks"})).unwrap();
    requests
        .send(crate::agent_tools::Request::Read(networks, reply))
        .unwrap();
    hub.update(cx, |hub, cx| hub.serve_tools(cx));
    let refused = answer.try_recv().unwrap().unwrap_err();
    assert!(
        refused.contains("not a member of any initiative"),
        "{refused}"
    );
}

#[gpui_kit::test]
fn closing_channels_closes_the_join_dialog(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, channel_events(), cx);
    cx.update(|window, cx| {
        board.update(cx, |board, cx| crate::join::open(board, window, cx));
    });
    board.read_with(cx, |board, _| assert!(board.join.is_some()));
    memberships(&hub, Some("fixture-user"), Ok(Vec::new()), cx);
    settle(cx);
    board.read_with(cx, |board, _| assert!(board.join.is_none()));
}
