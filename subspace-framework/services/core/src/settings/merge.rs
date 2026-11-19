use serde_json::{Map, Value};

use super::{ServiceSettings, SettingsError, TokenEdit, section, token};
use crate::config;

pub(super) fn validate(v: &ServiceSettings) -> Result<(), SettingsError> {
    validate_token(v)?;
    validate_repos(v)?;
    v.mcp.validate().map_err(|error| SettingsError::Validation {
        field: error.field,
        reason: error.reason,
    })
}

fn validate_token(v: &ServiceSettings) -> Result<(), SettingsError> {
    if matches!(&v.github_token, TokenEdit::Replace(secret) if secret.expose().is_empty()) {
        return Err(SettingsError::Validation {
            field: "token",
            reason: "replacement cannot be blank",
        });
    }
    Ok(())
}

fn validate_repos(v: &ServiceSettings) -> Result<(), SettingsError> {
    super::repos::validate(&v.repos)
}

pub(super) fn write_values(
    root: &mut Map<String, Value>,
    v: &ServiceSettings,
    github_token_present: bool,
) -> Result<(), SettingsError> {
    write_app_values(root, v)?;
    write_mcp(root, &v.mcp)?;
    write_github(root, &v.github_token, github_token_present);
    root.insert("repos".into(), super::repos::values(&v.repos));
    Ok(())
}

fn write_app_values(
    root: &mut Map<String, Value>,
    v: &ServiceSettings,
) -> Result<(), SettingsError> {
    let mut chat = section(root, "chat");
    let engine = serde_json::to_value(v.chat_engine)?;
    chat.insert("engine".into(), engine);
    write_acp_agent(&mut chat, v)?;
    root.insert("chat".into(), chat.into());
    write_board_order(root, &v.board_order);
    root.insert("autoRefresh".into(), Value::Bool(v.auto_refresh));
    write_terminal(root, &v.terminal);
    write_choices(root, "boards", &v.boards);
    write_choices(root, "features", &v.features);
    Ok(())
}

/// Sets the `terminal` object's keys, dropping a key set to `null`, and
/// leaves no empty object behind.
fn write_terminal(root: &mut Map<String, Value>, values: &Map<String, Value>) {
    let mut terminal = section(root, "terminal");
    for (key, value) in values {
        match value {
            Value::Null => terminal.remove(key),
            value => terminal.insert(key.clone(), value.clone()),
        };
    }
    if terminal.is_empty() {
        root.remove("terminal");
    } else {
        root.insert("terminal".into(), terminal.into());
    }
}

/// Writes `chat.acp.agent` once ACP is in use, keeping the section's other keys.
fn write_acp_agent(
    chat: &mut Map<String, Value>,
    v: &ServiceSettings,
) -> Result<(), SettingsError> {
    let mut acp = section(chat, "acp");
    let in_use = !acp.is_empty()
        || v.chat_engine == config::ChatEngine::Acp
        || v.acp_agent != config::AcpAgent::default();
    if in_use {
        acp.insert("agent".into(), serde_json::to_value(v.acp_agent)?);
        chat.insert("acp".into(), acp.into());
    }
    Ok(())
}

fn write_mcp(
    root: &mut Map<String, Value>,
    settings: &config::McpSettings,
) -> Result<(), SettingsError> {
    let mut mcp = section(root, "mcp");
    if let Value::Object(values) = serde_json::to_value(settings)? {
        mcp.extend(values);
    }
    root.insert("mcp".into(), mcp.into());
    Ok(())
}

fn write_github(root: &mut Map<String, Value>, edit: &TokenEdit, present: bool) {
    let mut github = section(root, "github");
    token(&mut github, edit, present);
    root.insert("github".into(), github.into());
}

fn write_choices(
    root: &mut Map<String, Value>,
    key: &str,
    choices: &std::collections::BTreeMap<String, bool>,
) {
    if choices.is_empty() {
        root.remove(key);
        return;
    }
    let values = choices
        .iter()
        .map(|(id, enabled)| (id.clone(), Value::Bool(*enabled)))
        .collect::<Map<String, Value>>();
    root.insert(key.into(), values.into());
}

pub(super) fn write_board_order(root: &mut Map<String, Value>, order: &[String]) {
    root.insert(
        "boardOrder".into(),
        Value::Array(order.iter().cloned().map(Value::String).collect()),
    );
}
