//! The MCP page's draft. Save turns it into `McpSettings` through the core
//! validation (REQ-04, REQ-05); the token file is never part of the draft.

use std::collections::BTreeMap;

use gpui_kit::component::input::InputState;
use gpui_kit::{App, AppContext as _, Entity, Window};
use subspace_core::config::{McpFieldError, McpSettings};
use subspace_core::mcp_token::TokenStatus;
use subspace_mcp::ToolGroup;

pub(super) const BIND_FIELD: &str = "mcp.bind";
pub(super) const PORT_FIELD: &str = "mcp.port";
pub(super) const TOKEN_FIELD: &str = "mcp.requireToken";

pub(super) struct McpDraft {
    pub(super) enabled: bool,
    pub(super) bind: Entity<InputState>,
    pub(super) port: Entity<InputState>,
    pub(super) require_token: bool,
    groups: BTreeMap<String, bool>,
    saved_groups: BTreeMap<String, bool>,
    /// The field that blocked the last Save, cleared by the next valid Save.
    pub(super) error: Option<McpFieldError>,
    pub(super) token: TokenStatus,
}

/// The draft's text and switches, as Save reads them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct McpFields {
    pub(super) enabled: bool,
    pub(super) bind: String,
    pub(super) port: String,
    pub(super) require_token: bool,
    pub(super) groups: BTreeMap<String, bool>,
}

impl McpDraft {
    pub(super) fn new(
        saved: &McpSettings,
        token: TokenStatus,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        Self {
            enabled: saved.enabled,
            bind: text_input(&saved.bind.to_string(), window, cx),
            port: text_input(&saved.port.to_string(), window, cx),
            require_token: saved.require_token,
            groups: saved.groups.clone(),
            saved_groups: saved.groups.clone(),
            error: None,
            token,
        }
    }

    pub(super) fn fields(&self, cx: &App) -> McpFields {
        McpFields {
            enabled: self.enabled,
            bind: self.bind.read(cx).value().to_string(),
            port: self.port.read(cx).value().to_string(),
            require_token: self.require_token,
            groups: self.groups.clone(),
        }
    }

    pub(super) fn group_enabled(&self, id: &str) -> bool {
        self.groups.get(id).copied().unwrap_or(true)
    }

    pub(super) fn set_group(&mut self, id: &str, enabled: bool) {
        set_group(&mut self.groups, &self.saved_groups, id, enabled);
    }

    /// A successful Save makes the written groups the new baseline.
    pub(super) fn accept(&mut self, saved: &McpSettings) {
        self.saved_groups.clone_from(&saved.groups);
        self.groups.clone_from(&saved.groups);
        self.error = None;
    }
}

fn text_input(text: &str, window: &mut Window, cx: &mut App) -> Entity<InputState> {
    let text = text.to_owned();
    cx.new(|cx| {
        let mut input = InputState::new(window, cx);
        input.set_value(text, window, cx);
        input
    })
}

/// Restores a saved entry when the switch returns to it, so switching a
/// group off and on again is not a change.
fn set_group(
    groups: &mut BTreeMap<String, bool>,
    saved: &BTreeMap<String, bool>,
    id: &str,
    enabled: bool,
) {
    match saved.get(id) {
        Some(value) if *value == enabled => {
            groups.insert(id.to_owned(), *value);
        }
        _ if enabled => {
            groups.remove(id);
        }
        _ => {
            groups.insert(id.to_owned(), false);
        }
    }
}

/// The policy Save writes, or the field that blocks it.
pub(super) fn draft_settings(
    saved: &McpSettings,
    fields: &McpFields,
) -> Result<McpSettings, McpFieldError> {
    let address = McpSettings::validate_fields(&fields.bind, &fields.port, fields.require_token)?;
    let mut settings = saved.clone();
    settings.enabled = fields.enabled;
    settings.bind = address.ip();
    settings.port = address.port();
    settings.require_token = fields.require_token;
    settings.groups.clone_from(&fields.groups);
    Ok(settings)
}

/// The message beside the marked field.
pub(super) fn field_message(error: &McpFieldError) -> String {
    match error.field {
        BIND_FIELD => "Enter an IPv4 or IPv6 address, such as 127.0.0.1.".to_owned(),
        PORT_FIELD => "Enter a port from 1 to 65535.".to_owned(),
        TOKEN_FIELD => {
            "A non-loopback bind address needs a token. Turn on Require a token.".to_owned()
        }
        _ => error.to_string(),
    }
}

/// The Settings error for a rejected Save. Nothing is written.
pub(super) fn save_error(error: &McpFieldError) -> String {
    format!("MCP settings were not saved. {}", field_message(error))
}

/// The groups as the page lists them, served by the draft switches. For the
/// saved switches the served tools equal the served catalog (REQ-26).
pub(super) fn page_groups(groups: &[ToolGroup], enabled: impl Fn(&str) -> bool) -> Vec<ToolGroup> {
    groups
        .iter()
        .cloned()
        .map(|mut group| {
            group.served = enabled(group.id);
            for tool in &mut group.tools {
                tool.served = group.served;
            }
            group
        })
        .collect()
}

/// The token row's state. Never the token value (REQ-09).
pub(super) fn token_state_text(status: TokenStatus) -> &'static str {
    match status {
        TokenStatus::Missing => "No token file",
        TokenStatus::Private => "Token file saved, readable only by you",
        TokenStatus::NotPrivate => "Other users can read the token file. Replace the token.",
    }
}

/// Create the first token, or replace an existing file.
pub(super) fn token_action_label(status: TokenStatus) -> &'static str {
    match status {
        TokenStatus::Missing => "Create token",
        TokenStatus::Private | TokenStatus::NotPrivate => "Replace token",
    }
}

#[cfg(test)]
#[path = "mcp_draft_tests.rs"]
mod tests;
