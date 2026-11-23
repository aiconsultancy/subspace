//! The board's settings in its own file, `irc/config.json` beside the
//! config file, with networks, highlight words and the captain notification switch.
//! Network configs, which may carry passwords, go to the runtime only.

use std::path::PathBuf;

use serde_json::{Map, Value};
use subspace_client_irc::NetworkConfig;

/// The board id, which also names the folder of its settings file.
pub(crate) const BOARD_KEY: &str = "irc";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Settings {
    pub networks: Vec<NetworkConfig>,
    /// Words that highlight a line besides our nick.
    pub highlights: Vec<String>,
    pub captain_notifications: bool,
    /// Why some networks could not be read, for the server-less empty state.
    pub error: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            networks: Vec::new(),
            highlights: Vec::new(),
            captain_notifications: true,
            error: None,
        }
    }
}

pub(crate) const CAPTAIN_NOTIFICATIONS: &str = "captainNotifications";
/// Why a saved network with an id in the account networks' namespace is
/// not used.
pub(crate) const RESERVED: &str =
    "ids starting with @ are kept for networks from your account; remove this network";

pub(crate) fn captain_notifications(settings: &Map<String, Value>) -> bool {
    settings
        .get(CAPTAIN_NOTIFICATIONS)
        .is_none_or(|value| value.as_bool() == Some(true))
}

/// Reads the saved Channels settings. A malformed network is skipped and
/// reported; the others still connect.
pub(crate) fn parse(settings: Option<&Map<String, Value>>) -> Settings {
    let Some(settings) = settings else {
        return Settings::default();
    };
    let mut errors = Vec::new();
    let highlights: Vec<String> = settings
        .get("highlights")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    let networks = settings
        .get("networks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(
            |(index, value)| match serde_json::from_value::<NetworkConfig>(value.clone()) {
                Ok(network) if crate::managed::reserved(&network.id.0) => {
                    errors.push(format!("networks[{index}]: {RESERVED}"));
                    None
                }
                Ok(network) => Some(network),
                Err(error) => {
                    errors.push(format!("networks[{index}]: {error}"));
                    None
                }
            },
        )
        .collect();
    Settings {
        networks,
        highlights,
        captain_notifications: captain_notifications(settings),
        error: (!errors.is_empty()).then(|| errors.join("; ")),
    }
}

/// The settings the shell loaded from the board's file, when it provided them.
pub(crate) fn load(cx: &gpui_kit::App) -> Settings {
    parse(
        subspace_framework::dashboard::services(cx)
            .and_then(|services| services.config.board_settings.get(BOARD_KEY)),
    )
}

/// The board's settings file on this computer, when the shell provided one.
pub(crate) fn file(cx: &gpui_kit::App) -> Option<PathBuf> {
    subspace_framework::dashboard::services(cx)
        .map(|services| services.board_config_file(BOARD_KEY))
}

/// The config file that enables boards, when the shell provided one.
pub(crate) fn product_file(cx: &gpui_kit::App) -> Option<PathBuf> {
    subspace_framework::dashboard::services(cx).map(|services| services.config_file.clone())
}

#[cfg(test)]
mod tests {
    use super::parse;
    use serde_json::json;

    #[test]
    fn networks_and_highlights_are_read_and_bad_entries_reported() {
        let value = json!({
            "networks": [
                {"id": "local", "host": "127.0.0.1", "port": 16667, "tls": false,
                 "nickname": "subspace", "channels": ["#subspace"]},
                {"id": "broken"}
            ],
            "highlights": ["subspace", 7]
        });
        let settings = parse(value.as_object());
        assert_eq!(settings.networks.len(), 1);
        assert_eq!(settings.networks[0].id.0, "local");
        assert_eq!(settings.networks[0].channels, ["#subspace"]);
        assert_eq!(settings.highlights, ["subspace"]);
        assert!(
            settings.networks[0].highlights.is_empty(),
            "board words highlight in the board, so changing them keeps connections"
        );
        let error = settings.error.unwrap_or_default();
        assert!(error.starts_with("networks[1]:"), "{error}");
    }

    #[test]
    fn a_saved_network_cannot_take_an_account_network_s_id() {
        let value = json!({"networks": [
            {"id": "@account.0123456789abcdef.0123456789abcdef", "host": "irc.example.com",
             "nickname": "ada"},
            {"id": "local", "host": "127.0.0.1", "nickname": "subspace"}
        ]});
        let settings = parse(value.as_object());
        assert_eq!(settings.networks.len(), 1);
        assert_eq!(settings.networks[0].id.0, "local");
        let error = settings.error.unwrap_or_default();
        assert!(
            error.starts_with("networks[0]: ids starting with @"),
            "{error}"
        );
    }

    #[test]
    fn missing_settings_mean_no_networks() {
        let settings = parse(None);
        assert!(settings.networks.is_empty());
        assert!(settings.error.is_none());
        assert!(parse(json!({}).as_object()).networks.is_empty());
    }
    #[test]
    fn captain_notifications_default_on_and_respect_the_saved_choice() {
        assert!(parse(None).captain_notifications);
        assert!(parse(json!({}).as_object()).captain_notifications);
        assert!(!parse(json!({"captainNotifications": false}).as_object()).captain_notifications);
        assert!(!parse(json!({"captainNotifications": "false"}).as_object()).captain_notifications);
    }
}
