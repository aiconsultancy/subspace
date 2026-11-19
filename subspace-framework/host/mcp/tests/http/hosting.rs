use super::{Http, common, meta, modern_headers, request, send};
use serde_json::{Value, json};
use subspace_core::config::McpSettings;
use subspace_mcp::{BindError, Endpoint, Host, HostConfig, HttpOptions, Identity};

fn options(address: std::net::SocketAddr) -> HttpOptions {
    HttpOptions {
        bind: address,
        token: None,
        origins: Vec::new(),
        hosts: Vec::new(),
    }
}

async fn host() -> Host {
    Host::start(
        &tokio::runtime::Handle::current(),
        HostConfig::new(
            options("127.0.0.1:0".parse().unwrap()),
            Identity::new("subspace", "Subspace", "test"),
        ),
        common::grouped(&McpSettings::default()).served_catalog(),
    )
    .await
    .unwrap()
}

async fn rpc(client: &Http, method: &str, name: Option<&str>) -> Value {
    let mut params = json!({"_meta":meta(json!({}))});
    if let Some(name) = name {
        params["name"] = json!(name);
    }
    let reply = send(
        client,
        http::Method::POST,
        &modern_headers(method, name),
        Some(request(1, method, params)),
    )
    .await;
    assert_eq!(reply.status, 200, "{}", reply.body());
    reply.events().remove(0)
}

#[tokio::test]
async fn host_swaps_groups_without_rebinding_and_stops_idempotently() {
    let mut host = host().await;
    let address = host.address();
    let client = Http::at(address);
    let before = rpc(&client, "tools/list", None).await;
    assert_eq!(before["result"]["tools"].as_array().unwrap().len(), 4);
    let mut settings = McpSettings::default();
    settings.groups.insert("test".into(), false);
    settings.groups.insert("application".into(), false);
    host.replace_catalog(common::grouped(&settings).served_catalog());
    let after = rpc(&client, "tools/list", None).await;
    assert!(after["result"]["tools"].as_array().unwrap().is_empty());
    let called = rpc(&client, "tools/call", Some("echo_read")).await;
    assert_eq!(called["error"]["code"], -32602);
    assert_eq!(host.address(), address);
    host.stop().await;
    host.stop().await;
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
    let rebound = tokio::net::TcpListener::bind(address).await.unwrap();
    assert_eq!(rebound.local_addr().unwrap(), address);
}

#[tokio::test]
async fn occupied_bind_is_typed_and_never_chooses_another_port() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let error = match Endpoint::bind(common::server(), &options(address)).await {
        Ok(_) => panic!("occupied bind succeeded"),
        Err(error) => error,
    };
    assert!(matches!(error, BindError::AddrInUse(_)));
    assert!(error.to_string().contains("Cannot listen"));
    assert!(error.to_string().contains(&address.to_string()));
}
