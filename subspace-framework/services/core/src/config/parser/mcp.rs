use serde_json::{Map, Value};

use super::Parser;
use crate::config::McpSettings;

pub(super) fn parse(parser: &mut Parser<'_>, section: Option<&Map<String, Value>>) -> McpSettings {
    let mut settings = McpSettings::default();
    let Some(section) = section else {
        return settings;
    };
    settings.enabled = boolean(parser, section, "enabled", false);
    settings.require_token = boolean(parser, section, "requireToken", false);
    if let Some(value) = section.get("bind") {
        match value.as_str().and_then(|text| text.parse().ok()) {
            Some(bind) => settings.bind = bind,
            None => parser.warn(
                "mcp.bind",
                "must be an IPv4 or IPv6 address; using the default",
            ),
        }
    }
    if let Some(value) = section.get("port") {
        match value
            .as_u64()
            .and_then(|port| u16::try_from(port).ok())
            .filter(|port| *port != 0)
        {
            Some(port) => settings.port = port,
            None => parser.warn(
                "mcp.port",
                "must be an integer from 1 to 65535; using the default",
            ),
        }
    }
    let groups = match section.get("groups") {
        None => None,
        Some(Value::Object(groups)) => Some(groups),
        Some(_) => {
            parser.warn("mcp.groups", "expected an object; using the defaults");
            None
        }
    };
    if let Some(groups) = groups {
        for (id, value) in groups {
            match value.as_bool() {
                Some(enabled) if !id.trim().is_empty() => {
                    settings.groups.insert(id.clone(), enabled);
                }
                _ => parser.warn(
                    format!("mcp.groups.{id}"),
                    "expected a group id and true or false; using the default",
                ),
            }
        }
    }
    if let Err(error) = settings.validate() {
        parser.warn(error.field, error.reason);
        settings.enabled = false;
        settings.bind = McpSettings::default().bind;
    }
    settings
}

fn boolean(
    parser: &mut Parser<'_>,
    section: &Map<String, Value>,
    key: &str,
    default: bool,
) -> bool {
    match section.get(key) {
        None => default,
        Some(Value::Bool(value)) => *value,
        Some(_) => {
            parser.warn(
                format!("mcp.{key}"),
                "expected true or false; using the default",
            );
            default
        }
    }
}
