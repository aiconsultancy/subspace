use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use serde::{Deserialize, Serialize};

/// Saved MCP policy. A missing group is served by default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
// qual:allow(srp, god_struct) reason: "Serialized MCP policy intentionally contains listener settings and its group choices in one config section."
pub struct McpSettings {
    pub enabled: bool,
    pub bind: IpAddr,
    pub port: u16,
    pub require_token: bool,
    pub groups: BTreeMap<String, bool>,
}

impl Default for McpSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 7331,
            require_token: false,
            groups: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{field}: {reason}")]
#[non_exhaustive]
pub struct McpFieldError {
    pub field: &'static str,
    pub reason: &'static str,
}

impl McpSettings {
    /// Validates saved policy, including the non-loopback authentication rule.
    pub fn validate(&self) -> Result<(), McpFieldError> {
        Self::validate_fields(
            &self.bind.to_string(),
            &self.port.to_string(),
            self.require_token,
        )
        .map(|_| ())
    }

    /// Validates text fields before a Settings draft becomes typed policy.
    pub fn validate_fields(
        bind: &str,
        port: &str,
        require_token: bool,
    ) -> Result<SocketAddr, McpFieldError> {
        let bind: IpAddr = bind.trim().parse().map_err(|_| McpFieldError {
            field: "mcp.bind",
            reason: "must be an IPv4 or IPv6 address",
        })?;
        let port = port
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|port| *port != 0)
            .ok_or(McpFieldError {
                field: "mcp.port",
                reason: "must be an integer from 1 to 65535",
            })?;
        if !bind.is_loopback() && !require_token {
            return Err(McpFieldError {
                field: "mcp.requireToken",
                reason: "a non-loopback address needs a token",
            });
        }
        Ok(SocketAddr::new(bind, port))
    }

    pub fn group_enabled(&self, id: &str) -> bool {
        self.groups.get(id).copied().unwrap_or(true)
    }

    pub fn address(&self) -> SocketAddr {
        SocketAddr::new(self.bind, self.port)
    }
}

#[cfg(test)]
mod tests {
    use super::McpSettings;

    #[test]
    fn parser_defaults_warns_and_keeps_safe_policy() {
        let parse = |text| crate::config::parse(text, std::path::Path::new("config.json"));
        assert_eq!(parse("{}").0.mcp, McpSettings::default());
        let (config, notices) = parse(
            r#"{"mcp":{"enabled":true,"bind":"::1","port":7400,"requireToken":true,"groups":{"application":false,"future":true}}}"#,
        );
        assert!(notices.is_empty());
        assert!(config.mcp.enabled);
        assert_eq!(config.mcp.address().to_string(), "[::1]:7400");
        assert!(!config.mcp.group_enabled("application"));
        for text in [
            r#"{"mcp":{"bind":"localhost"}}"#,
            r#"{"mcp":{"port":70000}}"#,
            r#"{"mcp":{"port":0}}"#,
            r#"{"mcp":{"bind":"0.0.0.0","enabled":true}}"#,
        ] {
            let (config, notices) = parse(text);
            assert!(!notices.is_empty());
            assert!(config.mcp.validate().is_ok());
            assert!(!config.mcp.enabled);
        }
    }

    #[test]
    fn defaults_are_local_off_and_groups_are_on() {
        let settings = McpSettings::default();
        assert!(!settings.enabled);
        assert!(!settings.require_token);
        assert!(settings.group_enabled("application"));
        assert_eq!(settings.address().to_string(), "127.0.0.1:7331");
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn draft_fields_reject_bad_addresses_ports_and_unsafe_policy() {
        for bind in ["127.0.0.1", "::1"] {
            for port in ["1", "65535"] {
                assert!(McpSettings::validate_fields(bind, port, false).is_ok());
            }
        }
        for port in ["0", "65536", "70000", "-1", "1.5", ""] {
            assert_eq!(
                McpSettings::validate_fields("::1", port, false)
                    .unwrap_err()
                    .field,
                "mcp.port"
            );
        }
        assert_eq!(
            McpSettings::validate_fields("localhost", "7331", true)
                .unwrap_err()
                .field,
            "mcp.bind"
        );
        assert_eq!(
            McpSettings::validate_fields("0.0.0.0", "7331", false)
                .unwrap_err()
                .field,
            "mcp.requireToken"
        );
        assert!(McpSettings::validate_fields("0.0.0.0", "7331", true).is_ok());
    }
}
