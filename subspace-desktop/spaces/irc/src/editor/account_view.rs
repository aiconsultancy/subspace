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

const SWITCH_NOTE: &str = "Save removes this network and its saved passwords from irc/config.json. Channels then connects to the server with your account.";

/// The account's networks as the page shows them, the saved network whose
/// switch to them waits for confirmation, and the saved networks switched,
/// which Save checks again.
#[derive(Default)]
pub(crate) struct Account {
    pub networks: AccountNetworks,
    pub switching: Option<u64>,
    pub switched: Vec<Switched>,
}

/// A saved network removed from the draft for the account's network on
/// its server, by the person signed in then.
pub(crate) struct Switched {
    /// Its id in the saved settings, if it was saved.
    pub id: Option<String>,
    pub title: String,
    pub host: String,
    pub port: u16,
    pub person: Option<String>,
}

impl Account {
    pub fn new(networks: AccountNetworks) -> Self {
        Self {
            networks,
            switching: None,
            switched: Vec::new(),
        }
    }

    /// Why Save must not drop a switched saved network, if it must not:
    /// the person who switched it is no longer signed in, their fleets
    /// cannot be confirmed now, or their account no longer has a network
    /// for that server. Asked of the page's copy and of `live`, the hub's
    /// account at the moment of Save, so a copy not yet refreshed after a
    /// sign-out cannot let Save through.
    pub fn refused_switch(&self, live: Option<&AccountNetworks>) -> Option<String> {
        let views = [Some(&self.networks), live];
        self.switched.iter().find_map(|switched| {
            let why = views
                .iter()
                .flatten()
                .find_map(|now| lost(now, switched))?;
            Some(format!(
                "“{}” was not switched to your account because {why}, so it is kept. Cancel keeps your settings as they were.",
                switched.title
            ))
        })
    }

    /// Forgets the switches a Save already applied: their saved networks
    /// are no longer among `saved`, so a later Save is not held to them.
    pub fn saved(&mut self, saved: &[subspace_client_irc::NetworkConfig]) {
        self.switched.retain(|switched| {
            switched
                .id
                .as_ref()
                .is_some_and(|id| saved.iter().any(|network| network.id.0 == *id))
        });
    }

    /// Takes `networks`; true when they changed, which also withdraws a
    /// switch waiting for confirmation.
    pub fn set(&mut self, networks: AccountNetworks) -> bool {
        let changed = self.networks != networks;
        if changed {
            self.switching = None;
        }
        self.networks = networks;
        changed
    }

    /// Whether the saved and account networks are listed apart.
    pub fn shown(&self) -> bool {
        self.networks.account.is_some()
    }

    /// The account's server that saved network `form` connects to, while
    /// the account brings networks.
    pub fn server(&self, form: &NetworkForm, cx: &App) -> Option<&Server> {
        let text = form.text(cx);
        let port = parse_port(&text.port)?;
        self.networks.account.as_ref()?;
        self.networks
            .servers
            .iter()
            .find(|server| same_server(&text.host, port, server))
    }
}

/// Why the switch of `switched` no longer holds for `now`, the account's
/// networks as one view sees them.
fn lost(now: &AccountNetworks, switched: &Switched) -> Option<&'static str> {
    let served = now
        .servers
        .iter()
        .any(|server| same_server(&switched.host, switched.port, server));
    match now.person.as_ref() {
        None => Some("you are signed out"),
        Some(person) if Some(person) != switched.person.as_ref() => {
            Some("another account is signed in")
        }
        Some(_) if !now.confirmed => Some("your fleets could not be confirmed just now"),
        Some(_) if !served => Some("your account no longer has a network for that server"),
        Some(_) => None,
    }
}

/// The section, while the signed-in account brings networks.
pub(super) fn section(
    editor: &IrcSettings,
    tokens: &Tokens,
    cx: &mut Context<IrcSettings>,
) -> Option<AnyElement> {
    let username = editor.account.networks.account.as_deref()?;
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
pub(super) fn intro(account: &str) -> String {
    let service = subspace_api_client::service_name();
    format!(
        "Channels connects to your fleets’ IRC servers as {account}, signed in with your {service} account. These networks follow your account: they can’t be edited here, and they go when you sign out."
    )
}

/// Under a saved network on one of the account's servers.
pub(super) fn offer_note() -> String {
    let service = subspace_api_client::service_name();
    format!("Your {service} account can sign in to this server for you.")
}

/// The heading and empty state of the saved networks: told apart from the
/// account's while those are shown.
pub(super) fn saved_words(apart: bool) -> (&'static str, &'static str) {
    if apart {
        ("Saved networks", "No saved networks yet.")
    } else {
        ("Networks", "No networks yet.")
    }
}

/// Removes saved network `key` for the account's network, once it still
/// connects to one of the account's servers; otherwise withdraws the
/// confirmation.
pub(super) fn switch(editor: &mut IrcSettings, key: u64, cx: &mut Context<IrcSettings>) {
    let switched = editor
        .network(key)
        .filter(|network| editor.account.server(network, cx).is_some())
        .and_then(|network| {
            let text = network.text(cx);
            Some(Switched {
                id: network.saved_id.clone(),
                title: network.title(cx),
                port: parse_port(&text.port)?,
                host: text.host,
                person: editor.account.networks.person.clone(),
            })
        })
        .filter(|_| editor.enabled && editor.account.switching == Some(key));
    match switched {
        Some(switched) => {
            editor.account.switched.push(switched);
            editor.remove(key, cx);
        }
        None => editor.ask_switch(None, cx),
    }
}

fn server_row(
    editor: &IrcSettings,
    index: usize,
    server: &Server,
    tokens: &Tokens,
    cx: &mut Context<IrcSettings>,
) -> AnyElement {
    let username = editor.account.networks.account.clone().unwrap_or_default();
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
    Some(fields::note(tokens, offer_note()).into_any_element())
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
    (title, key): (&str, u64),
    enabled: bool,
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
                    .disabled(!enabled)
                    .on_click(cx.listener(move |editor, _, _, cx| switch(editor, key, cx))),
                ),
        )
        .into_any_element()
}

pub(super) fn switch_title(title: &str) -> String {
    format!("Switch “{title}” to your account?")
}
