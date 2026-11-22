use super::NetworkConfig;
use super::config::{default_port, default_true};

#[test]
fn defaults_are_tls_and_auto_connect() {
    assert_eq!(default_port(), 6697);
    assert!(default_true());
}

#[test]
fn minimal_network_config_uses_defaults() {
    let config: NetworkConfig =
        serde_json::from_str(r#"{"id":"local","host":"127.0.0.1","nickname":"me"}"#).unwrap();
    assert_eq!(config.port, 6697);
    assert!(config.tls);
    assert!(config.auto_connect);
    assert!(config.highlights.is_empty());
    assert!(config.sasl.is_none());
}
