//! A network's connection settings, from the Channels Space's settings file.

use serde::{Deserialize, Serialize};

use super::NetworkId;

/// SASL credentials. Only `PLAIN` and `EXTERNAL` are offered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sasl {
    pub username: String,
    #[serde(default)]
    pub password: String,
    /// A file holding the password, read at each connect and never stored;
    /// used when `password` is empty.
    #[serde(default)]
    pub password_file: Option<std::path::PathBuf>,
    #[serde(default)]
    pub external: bool,
    /// Where to fetch the PLAIN login at each connect when there is neither
    /// a password nor a password file, such as a short-lived one another
    /// service issues. Set by the owner, never read from or written to the
    /// settings file.
    #[serde(skip)]
    pub credential: Option<super::CredentialSource>,
}

/// One network from `networks` in the Channels Space's settings file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkConfig {
    pub id: NetworkId,
    /// Display name; defaults to the id.
    #[serde(default)]
    pub name: String,
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_true")]
    pub tls: bool,
    /// Only for local test servers.
    #[serde(default)]
    pub accept_invalid_certs: bool,
    pub nickname: String,
    #[serde(default)]
    pub alt_nicks: Vec<String>,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub realname: String,
    /// Server password (`PASS`), not the account password.
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub sasl: Option<Sasl>,
    /// Channels joined after registration, such as `#subspace`.
    #[serde(default)]
    pub channels: Vec<String>,
    #[serde(default = "default_true")]
    pub auto_connect: bool,
    /// Words that highlight a line, besides our own nick (case-insensitive,
    /// whole words).
    #[serde(default)]
    pub highlights: Vec<String>,
    /// PEM CA certificate(s) trusted for this server, besides the system roots,
    /// such as a private network's own CA.
    #[serde(default)]
    pub ca_cert: Option<std::path::PathBuf>,
    /// PEM client certificate for TLS (and SASL `EXTERNAL`).
    #[serde(default)]
    pub client_cert: Option<std::path::PathBuf>,
    /// PEM private key for `client_cert`; defaults to the certificate file.
    #[serde(default)]
    pub client_key: Option<std::path::PathBuf>,
}

pub(super) fn default_port() -> u16 {
    6697
}

pub(super) fn default_true() -> bool {
    true
}
