//! One network's Settings draft as plain values: read from its saved JSON,
//! validated, and written back over that JSON so keys the form does not
//! show are kept. Pure data: no GPUI.

use serde_json::{Map, Value, json};
use subspace_client_irc::NetworkConfig;

use super::validate::parse_port;

/// The default port of a new network.
pub(crate) const DEFAULT_PORT: u16 = 6697;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum SaslMode {
    #[default]
    None,
    Plain,
    External,
}

impl SaslMode {
    pub const ALL: [Self; 3] = [Self::None, Self::Plain, Self::External];

    // qual:allow(dry, repeated_matches) reason: "This enum has one mapping of its own; rustqual groups unrelated enums only because their three-arm variant matches share one shape."
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Plain => "Account and password (PLAIN)",
            Self::External => "Client certificate (EXTERNAL)",
        }
    }

    pub fn is_plain(self) -> bool {
        self == Self::Plain
    }

    pub fn is_external(self) -> bool {
        self == Self::External
    }

    pub fn from_label(label: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|mode| mode.label() == label)
            .unwrap_or_default()
    }
}

/// The form's values, as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NetworkText {
    pub name: String,
    pub host: String,
    pub port: String,
    pub tls: bool,
    pub accept_invalid_certs: bool,
    pub ca_cert: String,
    pub client_cert: String,
    pub client_key: String,
    pub nickname: String,
    pub alt_nicks: String,
    pub username: String,
    pub realname: String,
    pub sasl: SaslMode,
    pub sasl_account: String,
    pub password_from_file: bool,
    pub password_file: String,
    pub channels: String,
    pub auto_connect: bool,
}

impl Default for NetworkText {
    fn default() -> Self {
        Self {
            name: String::new(),
            host: String::new(),
            port: DEFAULT_PORT.to_string(),
            tls: true,
            accept_invalid_certs: false,
            ca_cert: String::new(),
            client_cert: String::new(),
            client_key: String::new(),
            nickname: String::new(),
            alt_nicks: String::new(),
            username: String::new(),
            realname: String::new(),
            sasl: SaslMode::None,
            sasl_account: String::new(),
            password_from_file: false,
            password_file: String::new(),
            channels: String::new(),
            auto_connect: true,
        }
    }
}

/// The form's values for a saved network object. Passwords are not read.
pub(crate) fn from_saved(raw: &Map<String, Value>) -> NetworkText {
    let text = |key: &str| {
        raw.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let flag = |key: &str, default: bool| raw.get(key).and_then(Value::as_bool).unwrap_or(default);
    let sasl = raw.get("sasl").and_then(Value::as_object);
    let sasl_text = |key: &str| {
        sasl.and_then(|sasl| sasl.get(key))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    NetworkText {
        name: text("name"),
        host: text("host"),
        port: raw
            .get("port")
            .and_then(Value::as_u64)
            .unwrap_or(u64::from(DEFAULT_PORT))
            .to_string(),
        tls: flag("tls", true),
        accept_invalid_certs: flag("acceptInvalidCerts", false),
        ca_cert: text("caCert"),
        client_cert: text("clientCert"),
        client_key: text("clientKey"),
        nickname: text("nickname"),
        alt_nicks: list_text(raw.get("altNicks")),
        username: text("username"),
        realname: text("realname"),
        sasl: sasl_mode(sasl),
        sasl_account: sasl_text("username"),
        password_from_file: !sasl_text("passwordFile").is_empty()
            && sasl_text("password").is_empty(),
        password_file: sasl_text("passwordFile"),
        channels: list_text(raw.get("channels")),
        auto_connect: flag("autoConnect", true),
    }
}

fn sasl_mode(sasl: Option<&Map<String, Value>>) -> SaslMode {
    match sasl {
        None => SaslMode::None,
        Some(sasl) if sasl.get("external").and_then(Value::as_bool) == Some(true) => {
            SaslMode::External
        }
        Some(_) => SaslMode::Plain,
    }
}

/// Whether the saved object holds a server password, and a SASL password.
pub(crate) fn saved_passwords(raw: &Map<String, Value>) -> (bool, bool) {
    let present = |value: Option<&Value>| {
        value
            .and_then(Value::as_str)
            .is_some_and(|text| !text.is_empty())
    };
    let sasl = raw.get("sasl").and_then(|sasl| sasl.get("password"));
    (present(raw.get("password")), present(sasl))
}

fn list_text(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Items separated by commas, spaces or new lines.
pub(crate) fn split_list(text: &str) -> Vec<String> {
    text.split(|character: char| character == ',' || character.is_whitespace())
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A secret's edit on Save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SecretEdit {
    Keep,
    Remove,
    Replace(String),
}

impl SecretEdit {
    /// Typed text replaces; else a saved, uncleared secret is kept.
    pub fn new(saved: bool, clear: bool, typed: &str) -> Self {
        if !typed.is_empty() {
            Self::Replace(typed.to_owned())
        } else if saved && !clear {
            Self::Keep
        } else {
            Self::Remove
        }
    }

    pub fn is_set(&self) -> bool {
        !matches!(self, Self::Remove)
    }
}

/// The two secrets of a network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Secrets {
    pub server: SecretEdit,
    pub sasl: SecretEdit,
}

/// The saved object for `text`, written over `raw` so unknown keys stay.
pub(crate) fn to_json(
    raw: &Map<String, Value>,
    id: &str,
    text: &NetworkText,
    secrets: &Secrets,
) -> Map<String, Value> {
    let mut object = raw.clone();
    object.insert("id".into(), json!(id));
    set_text(&mut object, "name", &text.name);
    object.insert("host".into(), json!(text.host.trim()));
    object.insert(
        "port".into(),
        json!(parse_port(&text.port).unwrap_or(DEFAULT_PORT)),
    );
    object.insert("tls".into(), json!(text.tls));
    set_flag(
        &mut object,
        "acceptInvalidCerts",
        text.tls && text.accept_invalid_certs,
    );
    set_text(&mut object, "caCert", &text.ca_cert);
    set_text(&mut object, "clientCert", &text.client_cert);
    set_text(&mut object, "clientKey", &text.client_key);
    object.insert("nickname".into(), json!(text.nickname.trim()));
    set_list(&mut object, "altNicks", &text.alt_nicks);
    set_text(&mut object, "username", &text.username);
    set_text(&mut object, "realname", &text.realname);
    set_secret(&mut object, "password", &secrets.server);
    write_sasl(&mut object, text, &secrets.sasl);
    set_list(&mut object, "channels", &text.channels);
    object.insert("autoConnect".into(), json!(text.auto_connect));
    object
}

fn write_sasl(object: &mut Map<String, Value>, text: &NetworkText, password: &SecretEdit) {
    let mut sasl = object
        .get("sasl")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    sasl.insert("username".into(), json!(text.sasl_account.trim()));
    match text.sasl {
        SaslMode::None => {
            object.remove("sasl");
            return;
        }
        SaslMode::External => {
            sasl.insert("external".into(), json!(true));
            sasl.remove("password");
            sasl.remove("passwordFile");
        }
        SaslMode::Plain if text.password_from_file => {
            sasl.remove("external");
            sasl.remove("password");
            set_text(&mut sasl, "passwordFile", &text.password_file);
        }
        SaslMode::Plain => {
            sasl.remove("external");
            sasl.remove("passwordFile");
            set_secret(&mut sasl, "password", password);
        }
    }
    object.insert("sasl".into(), Value::Object(sasl));
}

fn set_text(object: &mut Map<String, Value>, key: &str, text: &str) {
    match text.trim() {
        "" => object.remove(key),
        text => object.insert(key.into(), json!(text)),
    };
}

fn set_flag(object: &mut Map<String, Value>, key: &str, on: bool) {
    if on {
        object.insert(key.into(), json!(true));
    } else {
        object.remove(key);
    }
}

fn set_list(object: &mut Map<String, Value>, key: &str, text: &str) {
    let items = split_list(text);
    if items.is_empty() {
        object.remove(key);
    } else {
        object.insert(key.into(), json!(items));
    }
}

fn set_secret(object: &mut Map<String, Value>, key: &str, edit: &SecretEdit) {
    match edit {
        SecretEdit::Keep => {}
        SecretEdit::Remove => {
            object.remove(key);
        }
        SecretEdit::Replace(secret) => {
            object.insert(key.into(), json!(secret));
        }
    }
}

/// The written object must read back as the runtime's config.
pub(crate) fn readable(object: &Map<String, Value>) -> Result<NetworkConfig, String> {
    serde_json::from_value(Value::Object(object.clone())).map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "draft_tests.rs"]
mod tests;
