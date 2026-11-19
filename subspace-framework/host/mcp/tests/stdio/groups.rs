use super::{Pipe, common, meta, request};
use serde_json::{Value, json};
use subspace_core::config::McpSettings;
use subspace_mcp::{Identity, ToolServer};

async fn list(pipe: &mut Pipe, method: &str) -> Value {
    pipe.call(request(1, method, json!({"_meta":meta(json!({}))})))
        .await
}

async fn instructions(server: &ToolServer) -> String {
    let mut pipe = Pipe::with_server(server.clone());
    let initialized = pipe
        .call(request(
            1,
            "initialize",
            json!({"protocolVersion":common::LEGACY,"capabilities":{},"clientInfo":{"name":"legacy","version":"1"}}),
        ))
        .await;
    initialized["result"]["instructions"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn initialize_describes_the_served_boards_and_follows_a_group_switch() {
    let server = ToolServer::new(
        common::grouped(&McpSettings::default()).served_catalog(),
        Identity::new("subspace", "Subspace", "test"),
    );
    let text = instructions(&server).await;
    assert!(text.starts_with("Subspace is the user's desktop work dashboard."));
    assert!(text.contains(
        "- Test board (test): Fixture notes. Tools: echo_read (read), notes_write (write)."
    ));
    assert!(text.contains("Shared tools: board_help (read"));
    assert!(text.contains("wait for the user's confirmation"));
    let mut settings = McpSettings::default();
    settings.groups.insert("test".into(), false);
    server.replace_catalog(common::grouped(&settings).served_catalog());
    let text = instructions(&server).await;
    assert!(text.contains("- Test board (test): Fixture notes. Its tools are turned off"));
    assert!(!text.contains("echo_read"));
}

#[tokio::test]
async fn disabled_groups_remove_tools_calls_and_application_resources_over_stdio() {
    let mut settings = McpSettings::default();
    settings.groups.insert("test".into(), false);
    settings.groups.insert("application".into(), false);
    let catalog = common::grouped(&settings);
    assert_eq!(catalog.groups()[0].tools.len(), 2);
    assert!(catalog.groups()[0].tools.iter().all(|tool| !tool.served));
    let mut pipe = Pipe::with_server(ToolServer::new(
        catalog.served_catalog(),
        Identity::new("subspace", "Subspace", "test"),
    ));
    let listed = list(&mut pipe, "tools/list").await;
    assert!(listed["result"]["tools"].as_array().unwrap().is_empty());
    let called = pipe
        .call(request(
            2,
            "tools/call",
            json!({"_meta":meta(json!({})),"name":"echo_read"}),
        ))
        .await;
    assert_eq!(called["error"]["code"], -32602);
    for (method, field) in [
        ("resources/list", "resources"),
        ("resources/templates/list", "resourceTemplates"),
    ] {
        let listed = list(&mut pipe, method).await;
        assert!(listed["result"][field].as_array().unwrap().is_empty());
    }
}

#[tokio::test]
async fn application_off_removes_help_and_search_but_cache_follows_its_board() {
    let dir = tempfile::tempdir().unwrap();
    let search =
        subspace_search::Service::open(dir.path().into(), "test".into(), vec!["test".into()])
            .unwrap();
    let mut settings = McpSettings::default();
    settings.groups.insert("application".into(), false);
    let grouped = common::grouped_with_search(&settings, Some(search.clone()));
    let server = ToolServer::new(
        grouped.served_catalog(),
        Identity::new("subspace", "Subspace", "test"),
    );
    let mut pipe = Pipe::with_server(server.clone());
    let resources = list(&mut pipe, "resources/list").await;
    assert!(
        resources["result"]["resources"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let templates = list(&mut pipe, "resources/templates/list").await;
    let templates = templates["result"]["resourceTemplates"].as_array().unwrap();
    assert_eq!(templates.len(), 1);
    assert_eq!(
        templates[0]["uriTemplate"],
        "subspace://cache/{board}/{account}/{id}"
    );
    settings.groups.insert("test".into(), false);
    server.replace_catalog(common::grouped_with_search(&settings, Some(search)).served_catalog());
    let templates = list(&mut pipe, "resources/templates/list").await;
    assert!(
        templates["result"]["resourceTemplates"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
