//! Where the API may serve initiative channels, a running hub holds its
//! networks at start until the first read of the memberships settles, so a
//! member's configured channels are not joined only to be left for their
//! initiative's, and someone in no initiative never connects. While held,
//! the pane says the initiatives are being checked.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_api_client::Error;
use subspace_api_client::initiatives::Catalog;
use subspace_api_client::model::MemberInitiative;
use subspace_client_irc::Command;

use super::super::{mount, mount_hub, settings};
use super::closed::{memberships, running};
use super::{catalogue, settle};
use crate::fixtures::network_id;
use crate::hub::{self, Hub};
use crate::initiatives::{READ_TIMEOUT, holds_for_first_read};

type Answer = (Option<String>, Result<Vec<MemberInitiative>, Error>);

/// A hub that starts by reading the memberships, which `answer` gives.
fn mount_reading(cx: &mut TestAppContext, answer: Answer) -> (Entity<Hub>, &mut VisualTestContext) {
    let settings = settings();
    let read = move |catalog: &mut Catalog, cx: &mut gpui_kit::Context<Catalog>| {
        catalog.refresh_for_test(None, move || answer, cx);
    };
    let (_board, hub, cx) = mount_hub(cx, move |cx| {
        hub::install_awaiting_read_for_test(&settings, read, cx)
    });
    settle(cx);
    (hub, cx)
}

fn signed_out() -> Answer {
    (None, Err(Error::SignInNeeded))
}

#[gpui_kit::test]
fn nothing_runs_while_the_first_read_is_in_flight(cx: &mut TestAppContext) {
    // At start the saved session is not restored yet: a refused read
    // settles nothing, and the networks keep waiting.
    let (hub, cx) = mount_reading(cx, signed_out());
    assert_eq!(running(&hub, cx), None, "held through a refused read");
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find("irc-checking-initiatives").visible());
    });
    // The session is restored and the memberships are read again: nothing
    // is configured while that read is in flight.
    let catalog = hub.read_with(cx, |hub, _| hub.initiatives.catalog.clone());
    catalog.update(cx, |catalog, cx| {
        catalog.refresh_for_test(None, || (Some("fixture-user".into()), Ok(catalogue())), cx);
        assert!(catalog.reading());
    });
    assert_eq!(running(&hub, cx), None, "in flight");
    settle(cx);
    hub.read_with(cx, |hub, _| {
        let sent: Vec<_> = hub
            .sent()
            .iter()
            .filter(|command| {
                matches!(command, Command::Configure(_) | Command::SetAutojoin { .. })
            })
            .collect();
        let initiative = Command::SetAutojoin {
            network: network_id(),
            channels: Some(vec!["#design".into(), "#releases".into()]),
        };
        assert!(matches!(sent[0], Command::Configure(networks) if networks.len() == 1));
        assert!(
            sent[1..].iter().all(|command| **command == initiative),
            "the configured channels are never joined first: {sent:?}"
        );
    });
}

#[gpui_kit::test]
fn someone_in_no_initiative_never_connects(cx: &mut TestAppContext) {
    let (hub, cx) = mount_reading(cx, (Some("fixture-user".into()), Ok(Vec::new())));
    hub.read_with(cx, |hub, _| {
        assert!(hub.initiatives.closed() && !hub.initiatives.awaiting_read());
        assert!(
            !hub.sent().iter().any(
                |command| matches!(command, Command::Configure(networks) if !networks.is_empty())
            ),
            "no network ever ran"
        );
    });
}

#[gpui_kit::test]
fn a_read_that_fails_lets_the_networks_run(cx: &mut TestAppContext) {
    let (hub, cx) = mount_reading(cx, (None, Err(Error::Network)));
    assert_eq!(running(&hub, cx), Some(1), "a failed read settles the wait");
}

#[gpui_kit::test]
fn a_read_that_never_settles_lets_the_networks_run_after_the_timeout(cx: &mut TestAppContext) {
    let (hub, cx) = mount_reading(cx, signed_out());
    let redraws = Rc::new(Cell::new(0));
    let counted = Rc::clone(&redraws);
    cx.update(|_, cx| {
        cx.observe(&hub, move |_, _| counted.set(counted.get() + 1))
            .detach();
    });
    cx.executor().advance_clock(READ_TIMEOUT);
    cx.run_until_parked();
    // The pane must not rely on a network event to stop saying it is
    // checking: with no network, none follows.
    assert!(redraws.get() > 0, "the end of the wait redraws the pane");
    settle(cx);
    assert_eq!(running(&hub, cx), Some(1));
    hub.read_with(cx, |hub, _| assert!(!hub.initiatives.awaiting_read()));
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("irc-checking-initiatives").is_none());
    });
}

#[gpui_kit::test]
fn a_hub_holds_only_when_running_with_initiative_channels_possible_and_unknown(
    cx: &mut TestAppContext,
) {
    assert!(holds_for_first_read(true, true, false));
    assert!(!holds_for_first_read(false, true, false), "not running");
    assert!(!holds_for_first_read(true, false, false), "never served");
    assert!(!holds_for_first_read(true, true, true), "already known");

    let (_board, hub, cx) = mount(cx, settings());
    let asked = Rc::new(Cell::new(0));
    let read = |asked: &Rc<Cell<u32>>| {
        let asked = Rc::clone(asked);
        move |_: &mut Catalog, _: &mut gpui_kit::Context<Catalog>| asked.set(asked.get() + 1)
    };
    hub.update(cx, |hub, cx| hub.await_first_read(false, read(&asked), cx));
    hub.read_with(cx, |hub, _| assert!(!hub.initiatives.awaiting_read()));
    memberships(&hub, Some("fixture-user"), Ok(catalogue()), cx);
    hub.update(cx, |hub, cx| hub.await_first_read(true, read(&asked), cx));
    hub.read_with(cx, |hub, _| assert!(!hub.initiatives.awaiting_read()));
    assert_eq!(
        asked.get(),
        0,
        "neither a stopped hub nor known memberships read again"
    );
}
