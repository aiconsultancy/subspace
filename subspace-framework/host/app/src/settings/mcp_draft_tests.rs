use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;
use subspace_agent_tools::{
    Access, AgentTool, ApprovalPolicy, Arguments, ReadService, ReadTool, ToolEnvironment, ToolSpec,
};
use subspace_core::config::McpSettings;
use subspace_core::mcp_token::TokenStatus;

use super::{
    BIND_FIELD, McpFields, PORT_FIELD, TOKEN_FIELD, draft_settings, field_message, page_groups,
    save_error, set_group, token_action_label, token_state_text,
};

fn fields(bind: &str, port: &str, require_token: bool) -> McpFields {
    McpFields {
        enabled: true,
        bind: bind.to_owned(),
        port: port.to_owned(),
        require_token,
        groups: BTreeMap::from([("github".to_owned(), false)]),
    }
}

#[test]
fn a_valid_draft_becomes_the_saved_policy() {
    let saved = McpSettings::default();
    let settings = draft_settings(&saved, &fields(" ::1 ", "7400", true)).unwrap();
    assert!(settings.enabled);
    assert_eq!(settings.address().to_string(), "[::1]:7400");
    assert!(settings.require_token);
    assert!(!settings.group_enabled("github"));
    assert!(settings.group_enabled("application"));
    assert!(settings.validate().is_ok());
}

#[test]
fn invalid_fields_name_the_field_and_explain_the_fix() {
    let saved = McpSettings::default();
    let cases = [
        (
            fields("localhost", "7331", false),
            BIND_FIELD,
            "IPv4 or IPv6",
        ),
        (
            fields("127.0.0.1", "70000", false),
            PORT_FIELD,
            "1 to 65535",
        ),
        (fields("127.0.0.1", "0", false), PORT_FIELD, "1 to 65535"),
        (
            fields("0.0.0.0", "7331", false),
            TOKEN_FIELD,
            "needs a token",
        ),
    ];
    for (draft, field, text) in cases {
        let error = draft_settings(&saved, &draft).unwrap_err();
        assert_eq!(error.field, field);
        assert!(field_message(&error).contains(text));
        assert!(save_error(&error).starts_with("MCP settings were not saved."));
    }
    assert!(draft_settings(&saved, &fields("0.0.0.0", "7331", true)).is_ok());
}

#[test]
fn switching_a_group_back_restores_the_saved_entry() {
    let saved = BTreeMap::from([("irc".to_owned(), true), ("actions".to_owned(), false)]);
    let mut groups = saved.clone();
    for (id, value) in [("irc", false), ("actions", true), ("github", false)] {
        set_group(&mut groups, &saved, id, value);
    }
    assert!(!groups["irc"]);
    assert!(!groups.contains_key("actions"));
    assert!(!groups["github"]);
    for (id, value) in [("irc", true), ("actions", false), ("github", true)] {
        set_group(&mut groups, &saved, id, value);
    }
    assert_eq!(groups, saved);
}

#[test]
fn the_token_row_never_needs_the_token_value() {
    assert_eq!(token_action_label(TokenStatus::Missing), "Create token");
    assert_eq!(token_action_label(TokenStatus::Private), "Replace token");
    assert_eq!(token_action_label(TokenStatus::NotPrivate), "Replace token");
    assert!(token_state_text(TokenStatus::NotPrivate).contains("Other users"));
    assert!(token_state_text(TokenStatus::Missing).contains("No token"));
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Lookup {
    Get,
}

struct Fixture;

impl ReadService for Fixture {
    type Request = Lookup;
    fn read(&self, _: Lookup) -> Result<Value, String> {
        Ok(Value::Null)
    }
}

fn read_tool(name: &'static str) -> Arc<dyn AgentTool> {
    let spec = ToolSpec {
        name,
        title: name,
        description: name,
        access: Access::Read,
        arguments: Arguments::Single { fields: Vec::new() },
        approval: ApprovalPolicy::None,
    };
    Arc::new(ReadTool::new(spec, Arc::new(Fixture)))
}

fn alpha_tools(_: &ToolEnvironment) -> Vec<Arc<dyn AgentTool>> {
    vec![read_tool("alpha_read")]
}

fn beta_tools(_: &ToolEnvironment) -> Vec<Arc<dyn AgentTool>> {
    vec![read_tool("beta_read")]
}

#[test]
fn the_page_marks_exactly_the_served_catalog_as_served() {
    let alpha = subspace_mcp::Board {
        id: "alpha",
        title: "Alpha",
        description: "",
        help: "# Alpha",
        help_images: &[],
        tools: alpha_tools,
    };
    let boards = [
        alpha,
        subspace_mcp::Board {
            id: "beta",
            title: "Beta",
            tools: beta_tools,
            ..alpha
        },
    ];
    let environment = subspace_agent_tools::test_support::environment(Default::default());
    for off in ["alpha", "beta", "application"] {
        let mut settings = McpSettings::default();
        settings.groups.insert(off.to_owned(), false);
        let catalog =
            subspace_mcp::GroupedCatalog::build(&boards, &[], &environment, &settings).unwrap();
        let page: BTreeSet<_> = page_groups(catalog.groups(), |id| settings.group_enabled(id))
            .iter()
            .flat_map(|group| &group.tools)
            .filter(|tool| tool.served)
            .map(|tool| tool.name)
            .collect();
        let served: BTreeSet<_> = catalog
            .served_catalog()
            .tools()
            .tools()
            .iter()
            .map(|tool| tool.spec().name)
            .collect();
        assert_eq!(page, served, "with {off} off");
        assert!(catalog.groups().iter().any(|group| group.id == off));
    }
}
