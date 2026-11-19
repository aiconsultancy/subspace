//! The setup actions of the empty board, and Settings saves applied
//! without a restart.

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use subspace_client_irc::{Command, ConnectionStatus};
use subspace_framework::dashboard::{Dashboard as _, ListDashboard as _};

use super::{channel_events, inject, mount, settings};
use crate::config::Settings;
use crate::fixtures::channel;

const CONFIG_FILE: &str = "/home/fixture/.config/subspace/config.json";
const IRC_FILE: &str = "/home/fixture/.config/subspace/irc/config.json";

/// The shell's services with `settings` loaded from the IRC settings file.
fn install_services(settings: serde_json::Value, cx: &mut gpui_kit::App) {
    let mut config = subspace_framework::core::config::Config::default();
    config.board_settings.insert(
        crate::config::BOARD_KEY.into(),
        settings.as_object().cloned().unwrap_or_default(),
    );
    cx.set_global(subspace_framework::dashboard::Services {
        credentials: std::sync::Arc::new(
            subspace_framework::core::credentials::Credentials::from_system(&config),
        ),
        config: std::sync::Arc::new(config),
        tools: subspace_framework::dashboard::Catalog::new(vec![]).unwrap(),
        search: None,
        config_file: CONFIG_FILE.into(),
    });
}

fn local_and_example() -> serde_json::Value {
    serde_json::json!({"networks": [
        {"id": "local", "name": "Local Ergo", "host": "127.0.0.1", "port": 16667,
         "tls": false, "nickname": "subspace", "channels": ["#subspace"]},
        {"id": "example", "name": "Example Network", "host": "irc.example.net",
         "nickname": "subspace"}
    ], "highlights": ["rocket"]})
}

#[gpui_kit::test]
fn without_networks_the_board_offers_settings_a_prompt_and_the_file(cx: &mut TestAppContext) {
    let (board, _, cx) = mount(cx, Settings::default());
    cx.update(|_, cx| install_services(serde_json::json!({}), cx));
    cx.update(|window, cx| window.render_frame(cx));
    cx.update(|window, _| {
        assert!(window.try_find("irc-unconfigured").is_some());
        assert!(window.try_find("irc-add-network").is_some());
        assert!(window.try_find("irc-reveal-config").is_some());
    });
    cx.update(|window, cx| window.click("irc-copy-prompt", cx));
    let copied = cx.update(|_, cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default()
    });
    assert!(
        copied.contains(&format!("IRC settings file: {IRC_FILE}")),
        "{copied}"
    );
    assert!(
        copied.contains(&format!("Subspace config file: {CONFIG_FILE}")),
        "{copied}"
    );
    cx.update(|_, cx| {
        assert_eq!(
            crate::config::file(cx),
            Some(std::path::PathBuf::from(IRC_FILE))
        );
    });
    assert!(copied.contains("`nickname`"));
    cx.update(|_, cx| {
        assert_eq!(board.read(cx).status(cx), None);
        assert_eq!(board.read(cx).badge(cx), None);
    });
}

#[gpui_kit::test]
fn saved_settings_reconfigure_the_runtime_and_keep_unchanged_buffers(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, channel_events(), cx);
    cx.update(|_, cx| install_services(local_and_example(), cx));
    cx.run_until_parked();
    hub.read_with(cx, |hub, _| {
        let configured = hub.sent().iter().rev().find_map(|command| match command {
            Command::Configure(networks) => Some(networks.len()),
            _ => None,
        });
        assert_eq!(configured, Some(2));
        let names: Vec<&str> = hub
            .state
            .networks()
            .iter()
            .map(|network| network.name.as_str())
            .collect();
        assert_eq!(names, ["Local Ergo", "Example Network"]);
        assert!(hub.state.buffer(&channel("#subspace")).is_some(), "kept");
        assert!(
            matches!(
                hub.state.networks()[0].status,
                ConnectionStatus::Connected { .. }
            ),
            "an unchanged network keeps its state"
        );
    });
    cx.update(|_, cx| {
        let services = cx.global_mut::<subspace_framework::dashboard::Services>();
        let mut config = (*services.config).clone();
        config.board_settings.insert(
            crate::config::BOARD_KEY.into(),
            serde_json::json!({"networks": [local_and_example()["networks"][1].clone()]})
                .as_object()
                .cloned()
                .unwrap_or_default(),
        );
        services.config = std::sync::Arc::new(config);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    hub.read_with(cx, |hub, _| {
        assert_eq!(hub.state.networks().len(), 1);
        assert!(
            hub.state.buffer(&channel("#subspace")).is_none(),
            "removed with its network"
        );
    });
    cx.update(|_, cx| {
        assert_eq!(
            board.read(cx).list().groups()[0].name.as_ref(),
            "Example Network"
        );
    });
}
