//! TCP and TLS connection acceptance tests.

use std::time::Duration;

use crate::connection::{self, Timeouts};
use crate::model::NetworkConfig;

fn config(json: serde_json::Value) -> NetworkConfig {
    serde_json::from_value(json).unwrap()
}

#[test]
fn n01_default_timeouts_are_fifteen_seconds_each() {
    let timeouts = Timeouts::default();
    assert_eq!(timeouts.tcp, Duration::from_secs(15));
    assert_eq!(timeouts.tls, Duration::from_secs(15));
}

#[tokio::test]
async fn n02_plain_connections_open_and_refusals_are_retryable() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let plain = config(serde_json::json!({
        "id": "t", "host": "127.0.0.1", "port": port, "tls": false, "nickname": "n"
    }));
    assert!(
        connection::connect(&plain, Timeouts::default())
            .await
            .is_ok()
    );
    drop(listener);
    let error = match connection::connect(&plain, Timeouts::default()).await {
        Ok(_) => panic!("the port is closed"),
        Err(error) => error,
    };
    assert!(error.retryable(), "{error}");
}

#[tokio::test]
async fn n03_a_stalled_tls_handshake_times_out_retryably() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let tls = config(serde_json::json!({
        "id": "t", "host": "127.0.0.1", "port": port, "tls": true,
        "acceptInvalidCerts": true, "nickname": "n"
    }));
    let timeouts = Timeouts {
        tcp: Duration::from_secs(5),
        tls: Duration::from_millis(100),
    };
    let accept = tokio::spawn(async move { listener.accept().await });
    let error = match connection::connect(&tls, timeouts).await {
        Ok(_) => panic!("the server never answers"),
        Err(error) => error,
    };
    assert!(error.retryable());
    assert_eq!(
        error.to_string(),
        "The TLS handshake timed out after 0 seconds"
    );
    drop(accept);
}

#[tokio::test]
async fn n04_unreadable_certificate_files_are_not_retryable() {
    for field in ["caCert", "clientCert"] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut json = serde_json::json!({
            "id": "t", "host": "127.0.0.1", "port": port, "tls": true, "nickname": "n"
        });
        json[field] = serde_json::json!("/no/such/cleanroom.pem");
        let accept = tokio::spawn(async move { listener.accept().await });
        let error = match connection::connect(&config(json), Timeouts::default()).await {
            Ok(_) => panic!("the file is missing"),
            Err(error) => error,
        };
        assert!(!error.retryable(), "{field}: {error}");
        assert!(error.to_string().starts_with("TLS: "), "{error}");
        drop(accept);
    }
}
