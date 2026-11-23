//! Settings > Channels edits networks, highlight words and captain
//! notifications in the board's own Save/Cancel draft.

mod account_view;
mod draft;
mod form;
pub(crate) mod ids;
mod network_view;
pub(crate) mod validate;
mod view;

#[cfg(test)]
mod tests;

use std::path::PathBuf;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{App, AppContext as _, Context, ElementId, Entity, Subscription, Window};
use serde_json::{Map, Value, json};
use subspace_framework::dashboard::settings::{BoardSettingsEditor, SettingsEditor};

use form::{NetworkForm, Source};
use validate::Problem;

/// Which part of the page shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Page {
    List,
    /// The form of the network with this key.
    Edit(u64),
}

pub(crate) struct IrcSettings {
    networks: Vec<NetworkForm>,
    highlights: Entity<InputState>,
    captain_notifications: bool,
    page: Page,
    /// The network whose removal waits for confirmation.
    removing: Option<u64>,
    /// The networks the signed-in account brings, and a saved network's
    /// switch to them.
    account: account_view::Account,
    /// The saved-password field whose locked input was last clicked.
    hint: Option<ElementId>,
    enabled: bool,
    next_key: u64,
    /// The board's settings file, which the page reveals.
    settings_file: Option<PathBuf>,
    _subscriptions: Vec<Subscription>,
}

/// The Settings editor for `saved`, the object in the board's settings file.
pub(crate) fn factory(
    saved: Map<String, Value>,
    enabled: bool,
    window: &mut Window,
    cx: &mut App,
) -> SettingsEditor {
    let settings_file = crate::config::file(cx);
    let editor = cx.new(|cx| IrcSettings::new(&saved, enabled, settings_file, window, cx));
    SettingsEditor::new(editor)
}

impl IrcSettings {
    fn new(
        saved: &Map<String, Value>,
        enabled: bool,
        settings_file: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let raws: Vec<Map<String, Value>> = saved
            .get("networks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_object)
            .cloned()
            .collect();
        let networks: Vec<NetworkForm> = raws
            .into_iter()
            .zip(0..)
            .map(|(raw, key)| NetworkForm::new(Source { key, raw }, window, cx))
            .collect();
        let words = string_list(saved.get("highlights")).join(", ");
        let highlights = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Separate with commas");
            input.set_value(words, window, cx);
            input
        });
        let mut subscriptions =
            vec![cx.subscribe(&highlights, |_, _, _: &InputEvent, cx| cx.notify())];
        let hub = crate::hub::entity(cx);
        let account = hub
            .as_ref()
            .map(|hub| hub.read(cx).account_networks())
            .unwrap_or_default();
        subscriptions.extend(hub.map(|hub| {
            cx.observe(&hub, |editor, hub, cx| {
                let account = hub.read(cx).account_networks();
                editor.set_account(account, cx);
            })
        }));
        Self {
            next_key: networks.len() as u64,
            networks,
            highlights,
            captain_notifications: crate::config::captain_notifications(saved),
            page: Page::List,
            removing: None,
            account: account_view::Account::new(account),
            hint: None,
            enabled,
            settings_file,
            _subscriptions: subscriptions,
        }
    }

    /// Shows `account`'s networks, when they changed.
    pub(crate) fn set_account(
        &mut self,
        account: crate::managed::AccountNetworks,
        cx: &mut Context<Self>,
    ) {
        if self.account.set(account) {
            cx.notify();
        }
    }

    /// The network whose form shows, if the page shows one.
    fn edited(&self) -> Option<&NetworkForm> {
        match self.page {
            Page::Edit(key) => self.networks.iter().find(|network| network.key == key),
            Page::List => None,
        }
    }

    fn is_locked(&self) -> bool {
        !self.enabled
    }

    fn is_hinted(&self, field: &ElementId) -> bool {
        self.hint.as_ref() == Some(field)
    }

    fn network(&self, key: u64) -> Option<&NetworkForm> {
        self.networks.iter().find(|network| network.key == key)
    }

    fn network_mut(&mut self, key: u64) -> Option<&mut NetworkForm> {
        self.networks.iter_mut().find(|network| network.key == key)
    }

    pub(crate) fn add_network(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let key = self.next_key;
        self.next_key += 1;
        let form = NetworkForm::new(
            Source {
                key,
                raw: Map::new(),
            },
            window,
            cx,
        );
        self.networks.push(form);
        self.open(key, cx);
    }

    pub(crate) fn open(&mut self, key: u64, cx: &mut Context<Self>) {
        self.removing = None;
        self.hint = None;
        self.page = Page::Edit(key);
        cx.notify();
    }

    /// Back to the list, unless the form still has problems to fix. A new
    /// network left blank is dropped.
    pub(crate) fn done(&mut self, key: u64, cx: &mut Context<Self>) {
        let Some(network) = self.network(key) else {
            self.page = Page::List;
            cx.notify();
            return;
        };
        if network.is_blank(cx) {
            self.networks.retain(|network| network.key != key);
            self.page = Page::List;
        } else if self.problems(key, cx).is_empty() {
            self.page = Page::List;
        } else if let Some(network) = self.network_mut(key) {
            network.checked = true;
        }
        cx.notify();
    }

    pub(crate) fn ask_remove(&mut self, key: Option<u64>, cx: &mut Context<Self>) {
        self.removing = key;
        self.account.switching = None;
        cx.notify();
    }

    /// Asks to confirm that saved network `key` gives way to the account's
    /// network on its server.
    pub(crate) fn ask_switch(&mut self, key: Option<u64>, cx: &mut Context<Self>) {
        self.account.switching = key;
        self.removing = None;
        cx.notify();
    }

    pub(crate) fn remove(&mut self, key: u64, cx: &mut Context<Self>) {
        self.networks.retain(|network| network.key != key);
        self.removing = None;
        self.account.switching = None;
        self.page = Page::List;
        cx.notify();
    }

    pub(crate) fn toggle_clear(
        &mut self,
        key: u64,
        sasl: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(network) = self.network_mut(key) else {
            return;
        };
        let secret = if sasl {
            &mut network.sasl_password
        } else {
            &mut network.server_password
        };
        secret.clear = !secret.clear;
        let input = secret.input.clone();
        input.update(cx, |input, cx| input.set_value("", window, cx));
        self.hint = None;
        cx.notify();
    }

    pub(crate) fn show_hint(&mut self, field: ElementId, cx: &mut Context<Self>) {
        self.hint = Some(field);
        cx.notify();
    }

    pub(crate) fn set_switch(
        &mut self,
        key: u64,
        set: impl FnOnce(&mut form::Switches),
        cx: &mut Context<Self>,
    ) {
        if let Some(network) = self.network_mut(key) {
            set(&mut network.switches);
            cx.notify();
        }
    }

    /// The problems of network `key`.
    pub(crate) fn problems(&self, key: u64, cx: &App) -> Vec<Problem> {
        self.network(key).map_or_else(Vec::new, |network| {
            validate::problems(&network.text(cx), &network.secrets(cx))
        })
    }

    /// The ids each network saves under: saved ids first, then new ones
    /// from their names, never repeated.
    fn ids(&self, cx: &App) -> Vec<String> {
        let mut taken: Vec<String> = self
            .networks
            .iter()
            .filter_map(|network| network.saved_id.clone())
            .collect();
        self.networks
            .iter()
            .map(|network| match &network.saved_id {
                Some(id) => id.clone(),
                None => {
                    let id = ids::new_id(&network.text(cx), &taken);
                    taken.push(id.clone());
                    id
                }
            })
            .collect()
    }

    fn network_values(&self, cx: &App) -> Result<Vec<Value>, String> {
        let ids = self.ids(cx);
        self.networks
            .iter()
            .zip(ids)
            .map(|(network, id)| {
                let text = network.text(cx);
                let secrets = network.secrets(cx);
                if let Some(problem) = validate::problems(&text, &secrets).first() {
                    return Err(format!("{}: {}", network.title(cx), problem.message));
                }
                let object = draft::to_json(&network.raw, &id, &text, &secrets);
                draft::readable(&object)
                    .map_err(|error| format!("{}: {error}", network.title(cx)))?;
                Ok(Value::Object(object))
            })
            .collect()
    }
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

impl BoardSettingsEditor for IrcSettings {
    fn snapshot(&self, cx: &App) -> Value {
        let networks: Vec<Value> = self
            .networks
            .iter()
            .map(|network| network.snapshot(cx))
            .collect();
        json!({
            "networks": networks,
            "highlights": self.highlights.read(cx).value().to_string(),
            (crate::config::CAPTAIN_NOTIFICATIONS): self.captain_notifications,
        })
    }

    fn values(&self, cx: &App) -> Result<Map<String, Value>, String> {
        let networks = self.network_values(cx)?;
        let highlights = draft::split_list(&self.highlights.read(cx).value());
        Ok(Map::from_iter([
            ("networks".to_owned(), Value::Array(networks)),
            ("highlights".to_owned(), json!(highlights)),
            (
                crate::config::CAPTAIN_NOTIFICATIONS.to_owned(),
                json!(self.captain_notifications),
            ),
        ]))
    }

    fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.enabled = enabled;
        cx.notify();
    }
}
