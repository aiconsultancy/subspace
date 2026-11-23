//! The account's networks on the Settings page: read-only rows for the
//! networks the signed-in account brings, and, on a saved network that
//! connects to one of their servers, the offer to switch it to the account.

use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Styled as _, TestSupportExt as _, div,
};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::fields;
use subspace_framework::lucide::IconName;

use super::IrcSettings;
use super::form::NetworkForm;
use super::validate::parse_port;
use crate::managed::{AccountNetworks, Server, same_server};

const OFFER_NOTE: &str = "Your Subspace account can sign in to this server for you.";
const SWITCH_NOTE: &str = "Save removes this network and its saved passwords from irc/config.json. Subspace then connects to the server with your account.";

/// The account's networks as the page shows them, and the saved network
/// whose switch to them waits for confirmation.
#[derive(Default)]
pub(crate) struct Account {
    pub networks: AccountNetworks,
    pub switching: Option<u64>,
}

impl Account {
    pub fn new(networks: AccountNetworks) -> Self {
        Self {
            networks,
            switching: None,
        }
    }

    /// Takes `networks`; true when they changed.
    pub fn set(&mut self, networks: AccountNetworks) -> bool {
        let changed = self.networks != networks;
        self.networks = networks;
        changed
    }

    /// The account's server that saved network `form` connects to, while
    /// the account brings networks.
    pub fn server(&self, form: &NetworkForm, cx: &App) -> Option<&Server> {
        let text = form.text(cx);
        let port = parse_port(&text.port)?;
        self.networks.username.as_ref()?;
        self.networks
            .servers
            .iter()
            .find(|server| same_server(&text.host, port, server))
    }
}

/// The section, while the signed-in account brings networks.
pub(super) fn section(
    editor: &IrcSettings,
    tokens: &Tokens,
    cx: &mut Context<IrcSettings>,
) -> Option<AnyElement> {
    let username = editor.account.networks.username.as_deref()?;
    Some(
        v_flex()
            .id("irc-account-networks")
            .test_support()
            .gap_3()
            .child(fields::heading("From your account", cx))
            .child(fields::note(tokens, intro(username)))
            .children(
                editor
                    .account
                    .networks
                    .servers
                    .iter()
                    .enumerate()
                    .map(|(index, server)| server_row(editor, index, server, tokens, cx)),
            )
            .into_any_element(),
    )
}

/// What the section says first: who the networks sign in as, and why
/// they cannot be edited.
pub(super) fn intro(username: &str) -> String {
    format!(
        "Subspace connects to your fleets’ IRC servers as {username}, signed in with your Subspace account. These networks follow your account: they can’t be edited here, and they go when you sign out."
    )
}

fn server_row(
    editor: &IrcSettings,
    index: usize,
    server: &Server,
    tokens: &Tokens,
    cx: &mut Context<IrcSettings>,
) -> AnyElement {
    let username = editor.account.networks.username.clone().unwrap_or_default();
    let manual = editor
        .networks
        .iter()
        .find(|network| editor.account.server(network, cx) == Some(server))
        .map(|network| network.title(cx));
    v_flex()
        .id(("irc-account-network", index))
        .test_support()
        .min_w_0()
        .gap_2()
        .pb_3()
        .border_b_1()
        .border_color(tokens.line)
        .child(div().font_weight(FontWeight::SEMIBOLD).child(server.name()))
        .child(fields::note(tokens, summary(server, &username)))
        .children(manual.map(|title| fields::note(tokens, kept(&title))))
        .into_any_element()
}

/// `host:port · TLS · nick`, as a saved network's row reads.
pub(super) fn summary(server: &Server, username: &str) -> String {
    let endpoint = &server.endpoint;
    let security = if endpoint.tls { "TLS" } else { "Plaintext" };
    format!(
        "{}:{} · {security} · {username}",
        endpoint.host, endpoint.port
    )
}

/// Why a server of the account has no network of its own.
pub(super) fn kept(title: &str) -> String {
    format!("Not connected: your network “{title}” connects to this server instead.")
}

/// Under a saved network on one of the account's servers: why it can
/// switch, with [`switch_button`] among its actions.
pub(super) fn offer(
    editor: &IrcSettings,
    network: &NetworkForm,
    tokens: &Tokens,
    cx: &App,
) -> Option<AnyElement> {
    editor.account.server(network, cx)?;
    Some(fields::note(tokens, OFFER_NOTE).into_any_element())
}

/// The action that asks to switch saved network `key` to the account.
pub(super) fn switch_button(
    key: u64,
    enabled: bool,
    cx: &mut Context<IrcSettings>,
) -> impl IntoElement {
    action_button(
        ("irc-switch-network", key),
        Intent::Neutral,
        Some(IconName::UserRound),
        "Use account sign-in…",
    )
    .disabled(!enabled)
    .on_click(cx.listener(move |editor, _, _, cx| editor.ask_switch(Some(key), cx)))
}

/// Confirms that saved network `key` gives way to the account's network.
pub(super) fn confirmation(
    title: &str,
    key: u64,
    tokens: &Tokens,
    cx: &mut Context<IrcSettings>,
) -> AnyElement {
    v_flex()
        .id(("irc-switch-confirmation", key))
        .test_support()
        .gap_2()
        .child(switch_title(title))
        .child(fields::note(tokens, SWITCH_NOTE))
        .child(
            h_flex()
                .gap_2()
                .child(
                    action_button(
                        ("irc-switch-cancel", key),
                        Intent::Neutral,
                        Some(IconName::X),
                        "Cancel",
                    )
                    .on_click(cx.listener(|editor, _, _, cx| editor.ask_switch(None, cx))),
                )
                .child(
                    action_button(
                        ("irc-switch-confirm", key),
                        Intent::Primary,
                        Some(IconName::UserRound),
                        "Switch",
                    )
                    .on_click(cx.listener(move |editor, _, _, cx| editor.remove(key, cx))),
                ),
        )
        .into_any_element()
}

pub(super) fn switch_title(title: &str) -> String {
    format!("Switch “{title}” to your account?")
}
