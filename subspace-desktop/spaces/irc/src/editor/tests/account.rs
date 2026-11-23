//! The account's networks on the Settings page: read-only, saying where
//! they come from, and the offer to switch a saved network on the same
//! server. Fictional accounts and servers only.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_framework::dashboard::settings::BoardSettingsEditor as _;

use super::super::{IrcSettings, account_view};
use super::{mount, saved};
use crate::initiatives::Endpoint;
use crate::managed::{AccountNetworks, Server};

/// Signed in as ada, with the aurora fleet on `host`.
fn account(host: &str) -> AccountNetworks {
    AccountNetworks {
        account: Some("ada".into()),
        servers: vec![Server {
            endpoint: Endpoint {
                host: host.into(),
                port: 6697,
                tls: true,
            },
            fleets: vec!["Aurora".into()],
        }],
    }
}

fn show(editor: &Entity<IrcSettings>, account: AccountNetworks, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| editor.set_account(account, cx));
        window.render_frame(cx);
    });
}

#[gpui_kit::test]
fn the_account_s_networks_show_read_only_with_where_they_come_from(cx: &mut TestAppContext) {
    let (editor, cx) = mount(cx, saved());
    cx.update(|window, _| assert!(window.try_find("irc-account-networks").is_none()));
    show(&editor, account("irc.example.com"), cx);
    cx.update(|window, _| {
        assert!(window.try_find("irc-account-networks").is_some());
        assert!(window.try_find(("irc-account-network", 0usize)).is_some());
        assert!(
            window.try_find(("irc-edit-network", 2u64)).is_none(),
            "nothing to edit or remove on the account's network"
        );
        assert!(window.try_find(("irc-switch-network", 0u64)).is_none());
    });
    cx.update(|_, cx| {
        let values = editor.read(cx).values(cx).unwrap_or_default();
        assert_eq!(
            values["networks"].as_array().map(Vec::len),
            Some(2),
            "Save writes only the saved networks"
        );
    });
    let intro = account_view::intro("ada");
    assert!(intro.contains("as ada") && intro.contains("your Subspace API account"));
    assert!(intro.contains("can’t be edited") && intro.contains("sign out"));
    let server = &account("irc.example.com").servers[0];
    assert_eq!(
        account_view::summary(server, "ada"),
        "irc.example.com:6697 · TLS · ada"
    );
}

#[gpui_kit::test]
fn signed_out_or_without_irc_sign_ins_the_page_is_as_before(cx: &mut TestAppContext) {
    let (editor, cx) = mount(cx, saved());
    show(&editor, AccountNetworks::default(), cx);
    cx.update(|window, _| {
        assert!(window.try_find("irc-account-networks").is_none());
        assert!(window.try_find(("irc-switch-network", 0u64)).is_none());
    });
}

#[gpui_kit::test]
fn a_saved_network_on_the_account_s_server_offers_to_switch(cx: &mut TestAppContext) {
    let (editor, cx) = mount(cx, saved());
    // The saved Example Network connects to irc.example.net:6697.
    show(&editor, account("IRC.example.net"), cx);
    cx.update(|window, _| {
        assert!(window.try_find(("irc-switch-network", 0u64)).is_some());
        assert!(
            window.try_find(("irc-switch-network", 1u64)).is_none(),
            "the local network is on another server"
        );
    });
    cx.update(|window, cx| window.click(("irc-switch-network", 0u64), cx));
    cx.update(|window, cx| window.render_frame(cx));
    cx.update(|window, _| {
        assert!(window.try_find(("irc-switch-confirmation", 0u64)).is_some());
        assert!(window.try_find(("irc-edit-network", 0u64)).is_none());
    });
    cx.update(|window, cx| window.click(("irc-switch-cancel", 0u64), cx));
    cx.update(|window, cx| window.render_frame(cx));
    cx.update(|window, _| {
        assert!(window.try_find(("irc-switch-confirmation", 0u64)).is_none());
    });
    cx.update(|window, cx| window.click(("irc-switch-network", 0u64), cx));
    cx.update(|window, cx| window.render_frame(cx));
    cx.update(|window, cx| window.click(("irc-switch-confirm", 0u64), cx));
    cx.update(|_, cx| {
        let editor = editor.read(cx);
        assert_eq!(editor.networks.len(), 1);
        let values = editor.values(cx).unwrap_or_default();
        assert_eq!(
            values["networks"][0]["id"], "local",
            "Save removes the saved network; the account's then connects"
        );
    });
}

#[test]
fn the_switch_names_the_network_and_why_one_is_not_connected() {
    assert_eq!(
        account_view::switch_title("Example Network"),
        "Switch “Example Network” to your account?"
    );
    assert_eq!(
        account_view::kept("Example Network"),
        "Not connected: your network “Example Network” connects to this server instead."
    );
}

#[gpui_kit::test]
fn a_saved_network_cannot_keep_an_account_network_s_id(cx: &mut TestAppContext) {
    let saved = serde_json::json!({"networks": [
        {"id": "@account.0123456789abcdef.0123456789abcdef", "name": "Taken",
         "host": "irc.example.com", "nickname": "ada"}
    ]})
    .as_object()
    .cloned()
    .unwrap_or_default();
    let (editor, cx) = mount(cx, saved);
    cx.update(|_, cx| {
        let editor = editor.read(cx);
        let problems = editor.problems(0, cx);
        assert_eq!(problems[0].message, crate::config::RESERVED);
        let refused = editor.values(cx).unwrap_err();
        assert!(
            refused.starts_with("Taken: ids starting with @"),
            "{refused}"
        );
    });
}

#[gpui_kit::test]
fn a_waiting_switch_is_withdrawn_when_the_account_or_server_changes(cx: &mut TestAppContext) {
    let (editor, cx) = mount(cx, saved());
    show(&editor, account("irc.example.net"), cx);
    cx.update(|window, cx| window.click(("irc-switch-network", 0u64), cx));
    cx.update(|window, cx| window.render_frame(cx));
    show(&editor, account("irc.example.org"), cx);
    cx.update(|window, _| {
        assert!(
            window.try_find(("irc-switch-confirmation", 0u64)).is_none(),
            "another account or server withdraws it"
        );
    });
    show(&editor, account("irc.example.net"), cx);
    cx.update(|window, cx| window.click(("irc-switch-network", 0u64), cx));
    cx.update(|window, cx| window.render_frame(cx));
    // The draft's server changes under the confirmation.
    super::type_into(&editor, 0, |inputs| &inputs.host, "irc.example.com", cx);
    cx.update(|window, cx| window.click(("irc-switch-confirm", 0u64), cx));
    cx.update(|_, cx| {
        let editor = editor.read(cx);
        assert_eq!(editor.networks.len(), 2, "checked again before removing");
        assert_eq!(editor.account.switching, None);
    });
}

#[gpui_kit::test]
fn a_locked_page_cannot_switch(cx: &mut TestAppContext) {
    let (editor, cx) = mount(cx, saved());
    show(&editor, account("irc.example.net"), cx);
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| editor.ask_switch(Some(0), cx));
        editor.update(cx, |editor, cx| editor.set_enabled(false, cx));
        window.render_frame(cx);
    });
    cx.update(|window, cx| window.click(("irc-switch-confirm", 0u64), cx));
    cx.update(|_, cx| assert_eq!(editor.read(cx).networks.len(), 2));
}

#[test]
fn the_saved_list_is_named_apart_from_the_account_s() {
    assert_eq!(
        account_view::saved_words(true),
        ("Saved networks", "No saved networks yet.")
    );
    assert_eq!(
        account_view::saved_words(false),
        ("Networks", "No networks yet.")
    );
    assert_eq!(
        account_view::offer_note(),
        "Your Subspace API account can sign in to this server for you."
    );
}
