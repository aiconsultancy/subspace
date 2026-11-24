//! The account's networks across the account's lifetime, through the real
//! timer path: a switch never plans from the previous account's fleets,
//! late answers for a forgotten account are dropped, a source refuses once
//! its account is gone, the fleets' servers come and go, and memberships
//! that could not be read are read again. A build with no API backend
//! brings none.

use gpui_kit::TestAppContext;

use super::support::{
    Api, configured, connect, connect_to, fleets, hosts, issued, member_of, person, settle,
};
use crate::tests::{mount, settings};

#[gpui_kit::test]
fn a_switch_never_plans_from_the_previous_account_s_fleets(cx: &mut TestAppContext) {
    let (_board, hub, cx) = mount(cx, settings());
    let api = Api::new(&["127.0.0.1", "127.0.0.2"]);
    let env = connect(&hub, &api, cx);
    member_of(&hub, (Some(1), "user-ada"), fleets("127.0.0.1", None), cx);
    settle(&hub, cx);
    assert_eq!(hosts(&configured(&hub, cx)), ["127.0.0.1", "127.0.0.1"]);
    // Bob signs in; the catalogue still holds Ada's fleets.
    api.sign_in(200, person("user-bob", Some("bob")));
    *env.generation.lock().unwrap() = Some(2);
    settle(&hub, cx);
    assert_eq!(
        hosts(&configured(&hub, cx)),
        ["127.0.0.1"],
        "nothing is planned from Ada's fleets for Bob"
    );
    assert_eq!(
        api.asked(),
        ["127.0.0.1"],
        "Bob's login never went to Ada's server"
    );
    member_of(&hub, (Some(2), "user-bob"), fleets("127.0.0.2", None), cx);
    settle(&hub, cx);
    let networks = configured(&hub, cx);
    assert_eq!(hosts(&networks), ["127.0.0.1", "127.0.0.2"]);
    assert_eq!(networks[1].nickname, "bob");
}

#[gpui_kit::test]
fn a_check_or_probe_answered_after_an_account_change_is_dropped(cx: &mut TestAppContext) {
    let (_board, hub, cx) = mount(cx, settings());
    let api = Api::new(&["127.0.0.1"]);
    let env = connect(&hub, &api, cx);
    member_of(&hub, (Some(1), "user-ada"), fleets("127.0.0.1", None), cx);
    // The check starts for generation 1, which ends before it answers.
    hub.update(cx, crate::managed::tick);
    *env.generation.lock().unwrap() = Some(2);
    cx.run_until_parked();
    assert_eq!(configured(&hub, cx).len(), 1, "the late check is dropped");
    hub.read_with(cx, |hub, cx| {
        assert!(hub.account_networks(cx).account.is_none())
    });
    // Signed in at generation 2, a probe starts, then the account changes.
    member_of(&hub, (Some(2), "user-ada"), fleets("127.0.0.1", None), cx);
    hub.update(cx, crate::managed::tick);
    cx.run_until_parked();
    hub.update(cx, crate::managed::tick);
    *env.generation.lock().unwrap() = Some(3);
    hub.update(cx, crate::managed::tick);
    cx.run_until_parked();
    assert_eq!(
        configured(&hub, cx).len(),
        1,
        "the probe answered for a forgotten account adds nothing"
    );
}

#[gpui_kit::test]
fn a_source_refuses_once_its_account_is_signed_out(cx: &mut TestAppContext) {
    let (_board, hub, cx) = mount(cx, settings());
    let api = Api::new(&["127.0.0.1"]);
    let env = connect(&hub, &api, cx);
    member_of(&hub, (Some(1), "user-ada"), fleets("127.0.0.1", None), cx);
    settle(&hub, cx);
    let networks = configured(&hub, cx);
    assert_eq!(issued(&networks).len(), 1);
    *env.generation.lock().unwrap() = Some(2);
    let source = networks[1]
        .sasl
        .as_ref()
        .unwrap()
        .credential
        .clone()
        .unwrap();
    let refused = subspace_client_irc::test_support::fetch(&source).unwrap_err();
    assert_eq!(refused, "Sign in under Settings › Account to connect");
    assert_eq!(api.asked().len(), 1, "nothing is asked for a gone account");
}

#[gpui_kit::test]
fn the_fleets_servers_come_and_go_with_the_catalogue(cx: &mut TestAppContext) {
    let (_board, hub, cx) = mount(cx, settings());
    let api = Api::new(&["127.0.0.1", "127.0.0.2"]);
    connect(&hub, &api, cx);
    member_of(&hub, (Some(1), "user-ada"), fleets("127.0.0.1", None), cx);
    settle(&hub, cx);
    assert_eq!(hosts(&configured(&hub, cx)), ["127.0.0.1", "127.0.0.1"]);
    member_of(
        &hub,
        (Some(1), "user-ada"),
        fleets("127.0.0.1", Some("127.0.0.2")),
        cx,
    );
    settle(&hub, cx);
    assert_eq!(
        hosts(&configured(&hub, cx)),
        ["127.0.0.1", "127.0.0.1", "127.0.0.2"]
    );
    member_of(&hub, (Some(1), "user-ada"), fleets("127.0.0.2", None), cx);
    settle(&hub, cx);
    let networks = configured(&hub, cx);
    assert_eq!(hosts(&networks), ["127.0.0.1", "127.0.0.2"]);
    assert_eq!(
        networks[1].name, "Aurora",
        "the fleet moved to the other server"
    );
    assert_eq!(api.asked().len(), 2, "each server was asked once");
}

#[gpui_kit::test]
fn memberships_that_were_not_read_for_the_account_are_read_again(cx: &mut TestAppContext) {
    let (_board, hub, cx) = mount(cx, settings());
    let api = Api::new(&["127.0.0.1"]);
    connect(&hub, &api, cx);
    let catalog = hub.read_with(cx, |hub, _| hub.initiatives.catalog.clone());
    catalog.update(cx, |catalog, cx| {
        catalog.set_memberships_for_test(
            Some("user-ada".into()),
            Err(subspace_api_client::Error::Network),
            cx,
        )
    });
    hub.update(cx, crate::managed::tick);
    cx.run_until_parked();
    let reads = |cx: &mut gpui_kit::VisualTestContext| {
        hub.read_with(cx, |hub, _| hub.managed.catalogue_reads_for_test())
    };
    assert_eq!(
        reads(cx),
        1,
        "read again at once after signing in, without Reload"
    );
    settle(&hub, cx);
    assert_eq!(reads(cx), 1, "then only after a growing wait");
    member_of(&hub, (Some(1), "user-ada"), fleets("127.0.0.1", None), cx);
    settle(&hub, cx);
    assert_eq!(reads(cx), 0, "known again");
    assert_eq!(configured(&hub, cx).len(), 2);
}

/// With no API backend the account is never checked: no request, no
/// credential asked for, no account network, and Settings shows the saved
/// networks as before, with nothing to switch to.
#[gpui_kit::test]
fn a_build_without_an_api_backend_brings_no_account_network(cx: &mut TestAppContext) {
    let (_board, hub, cx) = mount(cx, settings());
    let api = Api::new(&["127.0.0.1"]);
    connect_to(&hub, &api, false, cx);
    member_of(&hub, (Some(1), "user-ada"), fleets("127.0.0.1", None), cx);
    settle(&hub, cx);
    assert_eq!(
        hosts(&configured(&hub, cx)),
        ["127.0.0.1"],
        "only the saved network"
    );
    assert!(api.mock.requests().is_empty(), "nothing reached the API");
    assert!(api.asked().is_empty());
    let shown = hub.read_with(cx, |hub, cx| hub.account_networks(cx));
    assert_eq!(shown, crate::managed::AccountNetworks::default());
}
