//! One network's retained controls: the native inputs, switches and SASL
//! select that hold its draft. Saved passwords never enter an input; they
//! stay in `raw` until Save keeps, replaces or removes them.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::select::{SearchableVec, SelectEvent, SelectState};
use gpui_kit::{App, AppContext as _, Context, Entity, Subscription, Window};
use serde_json::{Map, Value, json};

use super::IrcSettings;
use super::draft::{self, NetworkText, SaslMode, SecretEdit, Secrets};

pub(crate) type Choice = Entity<SelectState<SearchableVec<String>>>;

/// A password input and its saved state.
pub(crate) struct Secret {
    pub input: Entity<InputState>,
    pub saved: bool,
    pub clear: bool,
}

impl Secret {
    fn edit(&self, cx: &App) -> SecretEdit {
        SecretEdit::new(self.saved, self.clear, &self.input.read(cx).value())
    }
}

/// The text inputs of a network, one per text field.
pub(crate) struct Inputs {
    pub name: Entity<InputState>,
    pub host: Entity<InputState>,
    pub port: Entity<InputState>,
    pub ca_cert: Entity<InputState>,
    pub client_cert: Entity<InputState>,
    pub client_key: Entity<InputState>,
    pub nickname: Entity<InputState>,
    pub alt_nicks: Entity<InputState>,
    pub username: Entity<InputState>,
    pub realname: Entity<InputState>,
    pub sasl_account: Entity<InputState>,
    pub password_file: Entity<InputState>,
    pub channels: Entity<InputState>,
}

impl Inputs {
    pub fn all(&self) -> [&Entity<InputState>; 13] {
        [
            &self.name,
            &self.host,
            &self.port,
            &self.ca_cert,
            &self.client_cert,
            &self.client_key,
            &self.nickname,
            &self.alt_nicks,
            &self.username,
            &self.realname,
            &self.sasl_account,
            &self.password_file,
            &self.channels,
        ]
    }
}

/// The switches of a network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Switches {
    pub tls: bool,
    pub accept_invalid_certs: bool,
    pub password_from_file: bool,
    pub auto_connect: bool,
}

impl Switches {
    /// The SASL password is typed and saved, not read from a file.
    pub fn stores_password(self) -> bool {
        !self.password_from_file
    }
}

pub(crate) struct NetworkForm {
    /// Stable identity for element ids, also for networks not saved yet.
    pub key: u64,
    /// The saved id; new networks get one from their name on Save.
    pub saved_id: Option<String>,
    /// The saved object, so Save keeps keys the form does not show.
    pub raw: Map<String, Value>,
    pub inputs: Inputs,
    pub switches: Switches,
    pub sasl: Choice,
    pub server_password: Secret,
    pub sasl_password: Secret,
    /// Show missing-value problems: the user chose Done, or Save failed.
    pub checked: bool,
    _subscriptions: Vec<Subscription>,
}

/// Where a form's values come from.
pub(crate) struct Source {
    pub key: u64,
    pub raw: Map<String, Value>,
}

impl NetworkForm {
    pub fn new(source: Source, window: &mut Window, cx: &mut Context<IrcSettings>) -> Self {
        let text = draft::from_saved(&source.raw);
        let (server_saved, sasl_saved) = draft::saved_passwords(&source.raw);
        let inputs = inputs(&text, window, cx);
        let sasl = sasl_choice(text.sasl, window, cx);
        let server_password = secret(server_saved, window, cx);
        let sasl_password = secret(sasl_saved, window, cx);
        let mut subscriptions: Vec<Subscription> = inputs
            .all()
            .into_iter()
            .chain([&server_password.input, &sasl_password.input])
            .map(|input| cx.subscribe(input, |_, _, _: &InputEvent, cx| cx.notify()))
            .collect();
        subscriptions.push(
            cx.subscribe(&sasl, |_, _, _: &SelectEvent<SearchableVec<String>>, cx| {
                cx.notify()
            }),
        );
        Self {
            key: source.key,
            saved_id: source
                .raw
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_owned),
            raw: source.raw,
            inputs,
            switches: Switches {
                tls: text.tls,
                accept_invalid_certs: text.accept_invalid_certs,
                password_from_file: text.password_from_file,
                auto_connect: text.auto_connect,
            },
            sasl,
            server_password,
            sasl_password,
            checked: false,
            _subscriptions: subscriptions,
        }
    }

    /// The draft's values, as typed.
    pub fn text(&self, cx: &App) -> NetworkText {
        let read = |input: &Entity<InputState>| input.read(cx).value().to_string();
        let inputs = &self.inputs;
        NetworkText {
            name: read(&inputs.name),
            host: read(&inputs.host),
            port: read(&inputs.port),
            tls: self.switches.tls,
            accept_invalid_certs: self.switches.accept_invalid_certs,
            ca_cert: read(&inputs.ca_cert),
            client_cert: read(&inputs.client_cert),
            client_key: read(&inputs.client_key),
            nickname: read(&inputs.nickname),
            alt_nicks: read(&inputs.alt_nicks),
            username: read(&inputs.username),
            realname: read(&inputs.realname),
            sasl: self.sasl_mode(cx),
            sasl_account: read(&inputs.sasl_account),
            password_from_file: self.switches.password_from_file,
            password_file: read(&inputs.password_file),
            channels: read(&inputs.channels),
            auto_connect: self.switches.auto_connect,
        }
    }

    /// The SASL password when `sasl`, else the server password.
    pub fn secret(&self, sasl: bool) -> &Secret {
        if sasl {
            &self.sasl_password
        } else {
            &self.server_password
        }
    }

    pub fn sasl_mode(&self, cx: &App) -> SaslMode {
        self.sasl
            .read(cx)
            .selected_value()
            .map_or(SaslMode::None, |label| SaslMode::from_label(label))
    }

    pub fn secrets(&self, cx: &App) -> Secrets {
        Secrets {
            server: self.server_password.edit(cx),
            sasl: self.sasl_password.edit(cx),
        }
    }

    /// The name shown for this network: its name, else its host.
    pub fn title(&self, cx: &App) -> String {
        let text = self.text(cx);
        [
            text.name,
            text.host,
            self.saved_id.clone().unwrap_or_default(),
        ]
        .into_iter()
        .map(|value| value.trim().to_owned())
        .find(|value| !value.is_empty())
        .unwrap_or_else(|| "New network".to_owned())
    }

    /// A new network the user has not typed into.
    pub fn is_blank(&self, cx: &App) -> bool {
        self.saved_id.is_none() && self.text(cx) == NetworkText::default()
    }

    /// Everything that changes the saved result, without secret text.
    pub fn snapshot(&self, cx: &App) -> Value {
        let text = self.text(cx);
        let typed = |secret: &Secret| !secret.input.read(cx).value().is_empty();
        json!({
            "id": self.saved_id,
            "text": [text.name, text.host, text.port, text.ca_cert, text.client_cert,
                     text.client_key, text.nickname, text.alt_nicks, text.username,
                     text.realname, text.sasl.label(), text.sasl_account, text.password_file,
                     text.channels],
            "switches": [text.tls, text.accept_invalid_certs, text.password_from_file,
                         text.auto_connect],
            "secrets": [self.server_password.clear, typed(&self.server_password),
                        self.sasl_password.clear, typed(&self.sasl_password)],
        })
    }
}

fn inputs(text: &NetworkText, window: &mut Window, cx: &mut App) -> Inputs {
    let mut input = |value: &str, placeholder: &str| text_input(value, placeholder, window, cx);
    Inputs {
        name: input(&text.name, "Example Network"),
        host: input(&text.host, "irc.example.net"),
        port: input(&text.port, ""),
        ca_cert: input(&text.ca_cert, "/path/to/ca.pem"),
        client_cert: input(&text.client_cert, "/path/to/client.pem"),
        client_key: input(&text.client_key, "The certificate file"),
        nickname: input(&text.nickname, ""),
        alt_nicks: input(&text.alt_nicks, "Separate with commas"),
        username: input(&text.username, "The nickname"),
        realname: input(&text.realname, "The nickname"),
        sasl_account: input(&text.sasl_account, ""),
        password_file: input(&text.password_file, "/path/to/password"),
        channels: input(&text.channels, "#subspace, #rust"),
    }
}

fn text_input(
    value: &str,
    placeholder: &str,
    window: &mut Window,
    cx: &mut App,
) -> Entity<InputState> {
    let value = value.to_owned();
    let placeholder = placeholder.to_owned();
    cx.new(|cx| {
        let mut input = InputState::new(window, cx).placeholder(placeholder);
        input.set_value(value, window, cx);
        input
    })
}

fn secret(saved: bool, window: &mut Window, cx: &mut App) -> Secret {
    Secret {
        input: cx.new(|cx| InputState::new(window, cx).masked(true)),
        saved,
        clear: false,
    }
}

fn sasl_choice(mode: SaslMode, window: &mut Window, cx: &mut App) -> Choice {
    let items: Vec<String> = SaslMode::ALL
        .into_iter()
        .map(|mode| mode.label().to_owned())
        .collect();
    let selected = mode.label().to_owned();
    cx.new(|cx| {
        let mut state = SelectState::new(SearchableVec::new(items), None, window, cx);
        state.set_selected_value(&selected, window, cx);
        state
    })
}
