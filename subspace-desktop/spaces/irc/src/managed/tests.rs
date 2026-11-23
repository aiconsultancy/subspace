//! The account's networks over a hub that records commands, through the
//! real timer path against an offline API: checked, probed per server,
//! run beside the saved networks, carrying the initiative's channels, each
//! signing in only with its own server's login, and gone on sign-out.
//! Fictional accounts and servers only.

mod lifecycle;
mod support;

use gpui_kit::{TestAppContext, VisualTestContext};
use serde_json::json;
use subspace_client_irc::Command;

use crate::fixtures::config;
use crate::tests::{mount, settings};
use support::{
    Api, INITIATIVE, configured, connect, fleets, hosts, issued, member_of, person, settle,
};

#[gpui_kit::test]
fn signing_in_connects_each_server_the_api_vouches_for_with_its_own_login(cx: &mut TestAppContext) {
    let (_board, hub, cx) = mount(cx, settings());
    let api = Api::new(&["127.0.0.1"]);
    connect(&hub, &api, cx);
    member_of(
        &hub,
        (Some(1), "user-ada"),
        fleets("127.0.0.1", Some("127.0.0.2")),
        cx,
    );
    settle(&hub, cx);
    let networks = configured(&hub, cx);
    assert_eq!(hosts(&networks), ["127.0.0.1", "127.0.0.1"]);
    let account = &networks[1];
    assert_eq!(
        (account.name.as_str(), account.nickname.as_str()),
        ("Aurora", "ada")
    );
    let sasl = account.sasl.as_ref().unwrap();
    let (name, device) = sasl.username.split_once('@').unwrap();
    assert_eq!((name, device.len()), ("ada", 16), "account@device");
    assert!(
        sasl.password.is_empty(),
        "nothing secret in the configuration"
    );
    let mut asked = api.asked();
    asked.sort();
    assert_eq!(
        asked,
        ["127.0.0.1", "127.0.0.2"],
        "one request per server, each naming its server; 127.0.0.2 is refused"
    );
    hub.read_with(cx, |hub, _| {
        assert_eq!(hub.saved.networks, [config()], "nothing is saved");
    });
}

#[gpui_kit::test]
fn a_login_goes_only_to_its_own_server_and_is_kept_under_reconnect_churn(cx: &mut TestAppContext) {
    let (_board, hub, cx) = mount(cx, settings());
    let api = Api::new(&["127.0.0.1", "127.0.0.2"]);
    connect(&hub, &api, cx);
    member_of(
        &hub,
        (Some(1), "user-ada"),
        fleets("127.0.0.1", Some("127.0.0.2")),
        cx,
    );
    settle(&hub, cx);
    let networks = configured(&hub, cx);
    assert_eq!(networks.len(), 3);
    for _ in 0..20 {
        for (host, username, password) in issued(&networks) {
            assert!(username.starts_with("ada@"));
            assert_eq!(
                password,
                format!("sirc1.for-{}", host.replace('.', "-")),
                "{host} gets its own server's login"
            );
        }
    }
    assert_eq!(
        api.asked().len(),
        2,
        "forty connects reuse the two logins the probes fetched"
    );
}

#[gpui_kit::test]
fn choosing_the_initiative_joins_its_channels_on_the_account_s_network(cx: &mut TestAppContext) {
    let (_board, hub, cx) = mount(cx, settings());
    let api = Api::new(&["127.0.0.1"]);
    connect(&hub, &api, cx);
    member_of(&hub, (Some(1), "user-ada"), fleets("127.0.0.1", None), cx);
    settle(&hub, cx);
    let id = configured(&hub, cx)[1].id.clone();
    let catalog = hub.read_with(cx, |hub, _| hub.initiatives.catalog.clone());
    catalog.update(cx, |catalog, cx| catalog.pick(INITIATIVE, cx));
    cx.run_until_parked();
    let joined = Command::SetAutojoin {
        network: id,
        channels: Some(vec!["#aurora-starbase".into(), "#aurora-nav".into()]),
    };
    hub.read_with(cx, |hub, _| {
        assert!(hub.sent().contains(&joined), "{:?}", hub.sent());
        assert_eq!(hub.missing_initiative_network(), None);
    });
}

#[gpui_kit::test]
fn saving_settings_keeps_the_account_s_network_running_as_it_was(cx: &mut TestAppContext) {
    let (_board, hub, cx) = mount(cx, settings());
    let api = Api::new(&["127.0.0.1"]);
    connect(&hub, &api, cx);
    member_of(&hub, (Some(1), "user-ada"), fleets("127.0.0.1", None), cx);
    settle(&hub, cx);
    let before = hub.read_with(cx, |hub, _| hub.sent().len());
    let mut saved = settings();
    saved.highlights.push("launch".into());
    hub.update(cx, |hub, cx| hub.apply(saved, cx));
    hub.read_with(cx, |hub, _| {
        assert!(
            !hub.sent()[before..]
                .iter()
                .any(|command| matches!(command, Command::Configure(_))),
            "a new highlight word reconnects nothing"
        );
        assert_eq!(hub.effective().networks.len(), 2);
    });
}

#[gpui_kit::test]
fn a_saved_network_on_the_fleet_s_server_is_kept_and_no_second_one_connects(
    cx: &mut TestAppContext,
) {
    let mut manual = config();
    manual.port = 6697;
    manual.tls = true;
    manual.accept_invalid_certs = true;
    let saved = crate::config::Settings {
        networks: vec![manual],
        ..settings()
    };
    let (_board, hub, cx) = mount(cx, saved);
    let api = Api::new(&["127.0.0.1"]);
    connect(&hub, &api, cx);
    member_of(&hub, (Some(1), "user-ada"), fleets("127.0.0.1", None), cx);
    settle(&hub, cx);
    hub.read_with(cx, |hub, _| {
        assert_eq!(hosts(&hub.effective().networks), ["127.0.0.1"]);
        assert_eq!(
            hub.account_networks().servers.len(),
            1,
            "Settings still offers the switch"
        );
    });
}

#[gpui_kit::test]
fn an_api_without_usernames_brings_no_network(cx: &mut TestAppContext) {
    let (_board, hub, cx) = mount(cx, settings());
    let api = Api::new(&["127.0.0.1"]);
    api.sign_in(200, person("user-ada", None));
    connect(&hub, &api, cx);
    member_of(&hub, (Some(1), "user-ada"), fleets("127.0.0.1", None), cx);
    settle(&hub, cx);
    assert_eq!(configured(&hub, cx).len(), 1);
    assert!(api.asked().is_empty(), "no credential is asked for");
}

#[gpui_kit::test]
fn settings_see_the_vouched_servers_only_while_signed_in(cx: &mut TestAppContext) {
    let (_board, hub, cx) = mount(cx, settings());
    let shown = |cx: &mut VisualTestContext| hub.read_with(cx, |hub, _| hub.account_networks());
    assert_eq!(shown(cx), crate::managed::AccountNetworks::default());
    let api = Api::new(&["127.0.0.1"]);
    let env = connect(&hub, &api, cx);
    member_of(
        &hub,
        (Some(1), "user-ada"),
        fleets("127.0.0.1", Some("127.0.0.2")),
        cx,
    );
    settle(&hub, cx);
    let networks = shown(cx);
    assert_eq!(networks.account.as_deref(), Some("ada"));
    assert_eq!(networks.servers.len(), 1, "only the server it vouches for");
    assert_eq!(networks.servers[0].name(), "Aurora");
    // Signing out: the account is forgotten at once.
    api.sign_in(401, json!({}));
    *env.generation.lock().unwrap() = Some(2);
    hub.update(cx, |hub, _| hub.managed.look_again_for_test());
    settle(&hub, cx);
    assert_eq!(shown(cx), crate::managed::AccountNetworks::default());
    assert_eq!(configured(&hub, cx).len(), 1, "the account's network went");
}
