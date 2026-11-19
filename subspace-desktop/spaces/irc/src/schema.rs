//! The documented shape of the IRC settings file: the one source the
//! agent prompt is written from. A test keeps it equal to the serde names of
//! `NetworkConfig` and `Sasl`.

/// One JSON key, its type, its default and what it means.
pub(crate) struct FieldDoc {
    pub key: &'static str,
    pub kind: &'static str,
    pub default: &'static str,
    pub meaning: &'static str,
}

const fn field(
    key: &'static str,
    kind: &'static str,
    default: &'static str,
    meaning: &'static str,
) -> FieldDoc {
    FieldDoc {
        key,
        kind,
        default,
        meaning,
    }
}

/// The top-level keys of the IRC settings file.
pub(crate) const BOARD: &[FieldDoc] = &[
    field(
        "networks",
        "array of network objects",
        "[]",
        "The networks to connect to, in the order the board lists them.",
    ),
    field(
        "highlights",
        "array of strings",
        "[]",
        "Words that mark a message as mentioning you on every network, besides your nick. Whole words, ignoring case.",
    ),
];

/// The keys of one network object.
pub(crate) const NETWORK: &[FieldDoc] = &[
    field(
        "id",
        "string",
        "required",
        "Stable, unique key, such as `libera`: lower case letters, digits and `-`. Do not change it for an existing network.",
    ),
    field(
        "name",
        "string",
        "the id",
        "Name shown as the network's group.",
    ),
    field("host", "string", "required", "Server host name or address."),
    field(
        "port",
        "integer 1-65535",
        "6697",
        "Server port; 6697 is TLS, 6667 is usually plaintext.",
    ),
    field("tls", "boolean", "true", "Connect with TLS."),
    field(
        "acceptInvalidCerts",
        "boolean",
        "false",
        "Accept any server certificate. Only for local test servers; never for a real network.",
    ),
    field(
        "caCert",
        "absolute file path",
        "none",
        "PEM CA certificate(s) trusted for this server besides the system roots, such as a private network's own CA.",
    ),
    field("nickname", "string", "required", "Preferred nick."),
    field(
        "altNicks",
        "array of strings",
        "[]",
        "Nicks to try when the preferred one is taken; else an underscore is appended.",
    ),
    field(
        "username",
        "string",
        "the nick",
        "User name sent at registration.",
    ),
    field(
        "realname",
        "string",
        "the nick",
        "Real name sent at registration.",
    ),
    field(
        "password",
        "string (secret)",
        "none",
        "Server password (`PASS`), not the account password. Most networks have none.",
    ),
    field(
        "sasl",
        "object",
        "none",
        "SASL sign-in with an account; see the `sasl` keys below. Omit it to connect without an account.",
    ),
    field(
        "channels",
        "array of strings",
        "[]",
        "Channels joined after every connect. Each starts with `#` or `&`, such as `#subspace`.",
    ),
    field(
        "autoConnect",
        "boolean",
        "true",
        "Connect when Subspace starts.",
    ),
    field(
        "highlights",
        "array of strings",
        "[]",
        "Extra highlight words for this network only.",
    ),
    field(
        "clientCert",
        "absolute file path",
        "none",
        "PEM client certificate for TLS, needed for SASL EXTERNAL.",
    ),
    field(
        "clientKey",
        "absolute file path",
        "the certificate file",
        "PEM private key for `clientCert`.",
    ),
];

/// The keys of a network's `sasl` object.
pub(crate) const SASL: &[FieldDoc] = &[
    field(
        "username",
        "string",
        "required",
        "Account name. May be empty for EXTERNAL.",
    ),
    field(
        "password",
        "string (secret)",
        "none",
        "Account password for SASL PLAIN.",
    ),
    field(
        "passwordFile",
        "absolute file path",
        "none",
        "File holding the account password, read at each connect; used when `password` is empty. Prefer it when the user keeps passwords in a file or a secrets manager mount.",
    ),
    field(
        "external",
        "boolean",
        "false",
        "Use SASL EXTERNAL with `clientCert` instead of a password. Needs TLS.",
    ),
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::{Value, json};
    use subspace_client_irc::{NetworkConfig, Sasl};

    use super::{FieldDoc, NETWORK, SASL};

    fn keys(docs: &[FieldDoc]) -> BTreeSet<&str> {
        docs.iter().map(|doc| doc.key).collect()
    }

    fn serde_keys(value: &Value) -> BTreeSet<String> {
        value
            .as_object()
            .map(|object| object.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Every field set, so serialisation names every key.
    fn complete() -> NetworkConfig {
        serde_json::from_value(json!({
            "id": "fixture", "host": "irc.example.net", "nickname": "subspace",
            "caCert": "/ca.pem", "clientCert": "/c.pem", "clientKey": "/k.pem",
            "sasl": {"username": "subspace", "passwordFile": "/pw"}
        }))
        .unwrap_or_else(|error| panic!("fixture: {error}"))
    }

    #[test]
    fn the_documented_keys_are_the_serde_keys() {
        let network = serde_json::to_value(complete()).unwrap_or_default();
        let documented: BTreeSet<String> = keys(NETWORK).into_iter().map(str::to_owned).collect();
        assert_eq!(documented, serde_keys(&network));
        let sasl: Option<Sasl> = complete().sasl;
        let sasl = serde_json::to_value(sasl).unwrap_or_default();
        let documented: BTreeSet<String> = keys(SASL).into_iter().map(str::to_owned).collect();
        assert_eq!(documented, serde_keys(&sasl));
    }
}
