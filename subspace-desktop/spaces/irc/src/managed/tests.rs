//! The account's networks over a hub that records commands: derived from
//! the signed-in account and its fleets, run beside the saved networks,
//! carrying the initiative's channels, and gone on sign-out. Fictional
//! accounts and servers only.

use std::sync::Arc;

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;
use subspace_api_client::Client;
use subspace_api_client::model::MemberInitiative;
use subspace_client_irc::{Command, NetworkConfig, NetworkId};

use crate::fixtures::config;
use crate::hub::Hub;
use crate::managed::{Account, SignedIn};
use crate::tests::{mount, settings};

const INITIATIVE: &str = "5d1f0c2e-5b7a-4f0e-9a51-2a8f3c6d7e10";

/// The aurora fleet's initiative, its IRC server on loopback over TLS, so
/// a test hub (which pairs initiatives with loopback networks) uses it.
fn aurora(host: &str) -> Vec<MemberInitiative> {
    serde_json::from_value(json!([{
        "id": INITIATIVE, "name": "Fallow Reach", "channels": "#aurora-nav",
        "fleetId": "aurora",
        "fleet": {"id": "aurora", "slug": "aurora", "name": "Aurora", "ircHost": host,
                  "ircPort": 6697, "ircTls": true, "starbaseChannel": "#aurora-starbase"}
    }]))
    .unwrap()
}

fn ada() -> Account {
    Account::SignedIn(SignedIn {
        username: "ada".into(),
        account: "ada".into(),
    })
}

fn sign_in(hub: &Entity<Hub>, account: Account, cx: &mut VisualTestContext) {
    let client = Client::loopback("http://127.0.0.1:9/api", None).unwrap();
    hub.update(cx, |hub, cx| {
        hub.sign_in_for_test(account, Some(Arc::new(client)), cx)
    });
    cx.run_until_parked();
}

fn member_of(hub: &Entity<Hub>, initiatives: Vec<MemberInitiative>, cx: &mut VisualTestContext) {
    let catalog = hub.read_with(cx, |hub, _| hub.initiatives.catalog.clone());
    catalog.update(cx, |catalog, cx| {
        catalog.set_memberships_for_test(Some("fixture-user".into()), Ok(initiatives), cx)
    });
    cx.run_until_parked();
}

/// The networks the runtime was last configured with.
fn configured(hub: &Entity<Hub>, cx: &mut VisualTestContext) -> Vec<NetworkConfig> {
    hub.read_with(cx, |hub, _| {
        hub.sent()
            .iter()
            .rev()
            .find_map(|command| match command {
                Command::Configure(networks) => Some(networks.clone()),
                _ => None,
            })
            .unwrap_or_default()
    })
}

fn ids(networks: &[NetworkConfig]) -> Vec<String> {
    networks
        .iter()
        .map(|network| network.id.0.clone())
        .collect()
}

#[gpui_kit::test]
fn a_signed_in_account_connects_to_its_fleet_s_server_beside_the_saved_networks(
    cx: &mut TestAppContext,
) {
    let (_board, hub, cx) = mount(cx, settings());
    member_of(&hub, aurora("127.0.0.1"), cx);
    assert_eq!(ids(&configured(&hub, cx)), ["local"], "not before sign-in");
    sign_in(&hub, ada(), cx);
    let networks = configured(&hub, cx);
    assert_eq!(ids(&networks), ["local", "ada-127-0-0-1-6697"]);
    let managed = &networks[1];
    assert_eq!(managed.name, "Aurora");
    assert_eq!(managed.nickname, "ada");
    let sasl = managed.sasl.as_ref().unwrap();
    let (account, device) = sasl.username.split_once('@').unwrap();
    assert_eq!((account, device.len()), ("ada", 16), "account@device");
    assert!(sasl.password.is_empty() && sasl.credential.is_some());
    hub.read_with(cx, |hub, _| {
        assert_eq!(hub.saved.networks, [config()], "nothing is saved");
        assert!(
            hub.state
                .networks()
                .iter()
                .any(|network| network.name == "Aurora")
        );
    });
}

#[gpui_kit::test]
fn choosing_the_initiative_joins_its_channels_on_the_account_s_network(cx: &mut TestAppContext) {
    let (_board, hub, cx) = mount(cx, settings());
    sign_in(&hub, ada(), cx);
    member_of(&hub, aurora("127.0.0.1"), cx);
    let catalog = hub.read_with(cx, |hub, _| hub.initiatives.catalog.clone());
    catalog.update(cx, |catalog, cx| catalog.pick(INITIATIVE, cx));
    cx.run_until_parked();
    let joined = Command::SetAutojoin {
        network: NetworkId("ada-127-0-0-1-6697".into()),
        channels: Some(vec!["#aurora-starbase".into(), "#aurora-nav".into()]),
    };
    hub.read_with(cx, |hub, _| {
        assert!(hub.sent().contains(&joined), "{:?}", hub.sent());
        assert_eq!(hub.missing_initiative_network(), None);
    });
}

#[gpui_kit::test]
fn signing_out_removes_the_account_s_networks_and_keeps_the_saved_ones(cx: &mut TestAppContext) {
    let (_board, hub, cx) = mount(cx, settings());
    member_of(&hub, aurora("127.0.0.1"), cx);
    sign_in(&hub, ada(), cx);
    assert_eq!(configured(&hub, cx).len(), 2);
    sign_in(&hub, Account::Without, cx);
    assert_eq!(ids(&configured(&hub, cx)), ["local"]);
    hub.read_with(cx, |hub, _| {
        assert!(
            !hub.state
                .networks()
                .iter()
                .any(|network| network.name == "Aurora")
        );
    });
}

#[gpui_kit::test]
fn saving_settings_keeps_the_account_s_network_running_as_it_was(cx: &mut TestAppContext) {
    let (_board, hub, cx) = mount(cx, settings());
    member_of(&hub, aurora("127.0.0.1"), cx);
    sign_in(&hub, ada(), cx);
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
    member_of(&hub, aurora("127.0.0.1"), cx);
    sign_in(&hub, ada(), cx);
    hub.read_with(cx, |hub, _| {
        assert_eq!(ids(&hub.effective().networks), ["local"]);
        let configures = hub
            .sent()
            .iter()
            .filter(|command| matches!(command, Command::Configure(_)))
            .count();
        assert_eq!(configures, 1, "only the start's: nothing changes");
    });
}

#[gpui_kit::test]
fn an_api_without_irc_sign_ins_brings_no_network(cx: &mut TestAppContext) {
    let (_board, hub, cx) = mount(cx, settings());
    member_of(&hub, aurora("127.0.0.1"), cx);
    sign_in(&hub, Account::Without, cx);
    hub.read_with(cx, |hub, _| {
        assert_eq!(ids(&hub.effective().networks), ["local"]);
    });
}
