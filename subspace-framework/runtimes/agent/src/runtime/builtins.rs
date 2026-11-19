//! The CLI's own tools Subspace lets Copilot use.
//!
//! The list comes from `tools.list` at startup, so a CLI that adds a tool
//! does not need a Subspace release. Discovery can fail or answer with an
//! older shape, so [`FALLBACK_BUILTIN_TOOLS`] holds the verified baseline tools.

use super::{Client, ToolsListRequest};

/// The baseline built-in tools supported by the bundled CLI. Used only when
/// discovery fails; keep it in step with the CLI the app pins.
pub(super) const FALLBACK_BUILTIN_TOOLS: &[&str] = &[
    "bash",
    "read_bash",
    "stop_bash",
    "list_bash",
    "glob",
    "grep",
    "create",
    "edit",
    "skill",
    "view",
    "web_fetch",
    "task",
    "read_agent",
    "list_agents",
    "write_agent",
];

/// Tools that drive the CLI's own terminal interface or its cross-session
/// state. Subspace draws the chat itself, so these have nothing to act on.
const TERMINAL_UI_TOOLS: &[&str] = &[
    "exit_plan_mode",
    "task_complete",
    "context_board",
    "send_inbox",
];

/// `ask_user` is added with the service tools, so it is not taken from here.
const SEPARATELY_ENABLED: &str = "ask_user";

pub(super) fn usable_builtin(name: &str) -> bool {
    !name.is_empty()
        && name != SEPARATELY_ENABLED
        && !TERMINAL_UI_TOOLS.contains(&name)
        && !name.starts_with("canvas")
        && ![
            "list_canvas_capabilities",
            "open_canvas",
            "invoke_canvas_action",
        ]
        .contains(&name)
}

/// The usable names of a discovered list, in the CLI's order and without
/// repeats. An empty result falls back to the pinned list.
pub(super) fn builtin_tools(listed: Vec<String>) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for name in listed.into_iter().filter(|name| usable_builtin(name)) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    if names.is_empty() {
        return fallback_builtin_tools();
    }
    names
}

fn fallback_builtin_tools() -> Vec<String> {
    FALLBACK_BUILTIN_TOOLS
        .iter()
        .filter(|name| usable_builtin(name))
        .map(|name| (*name).to_owned())
        .collect()
}

/// Asks the CLI which built-in tools it has. A failure is not fatal: the
/// pinned list keeps chat working against the CLI the app ships with.
pub(super) async fn discover_builtin_tools(client: &Client) -> Vec<String> {
    let listed = client
        .rpc()
        .tools()
        .list(ToolsListRequest::default())
        .await
        .map(|list| list.tools.into_iter().map(|tool| tool.name).collect())
        .unwrap_or_default();
    builtin_tools(listed)
}

#[cfg(test)]
mod tests {
    use super::{FALLBACK_BUILTIN_TOOLS, builtin_tools, usable_builtin};

    #[test]
    fn the_shell_file_and_agent_tools_are_available() {
        let tools = builtin_tools(
            FALLBACK_BUILTIN_TOOLS
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
        );
        for name in [
            "bash",
            "read_bash",
            "stop_bash",
            "list_bash",
            "glob",
            "grep",
            "create",
            "edit",
            "skill",
            "view",
            "web_fetch",
            "task",
            "read_agent",
            "list_agents",
            "write_agent",
        ] {
            assert!(tools.iter().any(|tool| tool == name), "{name}");
        }
        assert_eq!(tools.len(), FALLBACK_BUILTIN_TOOLS.len());
    }

    #[test]
    fn terminal_interface_tools_and_repeats_are_left_out() {
        for name in [
            "exit_plan_mode",
            "task_complete",
            "context_board",
            "send_inbox",
            "canvas_open",
            "ask_user",
            "",
        ] {
            assert!(!usable_builtin(name), "{name}");
        }
        let tools = builtin_tools(vec![
            "bash".into(),
            "exit_plan_mode".into(),
            "bash".into(),
            "a_new_cli_tool".into(),
        ]);
        assert_eq!(tools, ["bash", "a_new_cli_tool"]);
    }

    #[test]
    fn a_failed_discovery_keeps_the_pinned_list() {
        assert_eq!(
            builtin_tools(Vec::new()).len(),
            FALLBACK_BUILTIN_TOOLS.len()
        );
        assert_eq!(
            builtin_tools(vec!["exit_plan_mode".into()]).len(),
            FALLBACK_BUILTIN_TOOLS.len()
        );
    }
}
