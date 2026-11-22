use serde_json::{Value, json};
use subspace_framework::agent_tools::{Access, ApprovalPolicy, ToolSpec, schema};

fn text(name: &'static str) -> (&'static str, Value, bool) {
    (name, json!({"type":"string"}), true)
}
fn object(name: &'static str) -> (&'static str, Value, bool) {
    (name, json!({"type":"object"}), true)
}
fn source_revision() -> (&'static str, Value, bool) {
    (
        "expected",
        json!({"type":"object", "additionalProperties":false, "required":["definition","source"], "properties":{"definition":{"type":"string"},"source":{"type":"string"}}}),
        true,
    )
}
fn input_revision() -> (&'static str, Value, bool) {
    (
        "expected",
        json!({"type":"object", "additionalProperties":false, "required":["session_id","input_revision"], "properties":{"session_id":{"type":"string"},"input_revision":{"type":"integer","minimum":0}}}),
        true,
    )
}
fn read_actions() -> Vec<schema::Action> {
    vec![
        schema::action("schemas", "Get project and protocol schemas", vec![]),
        schema::action("list", "List project definitions and revisions", vec![]),
        schema::action(
            "project",
            "Read a definition and revision",
            vec![text("id")],
        ),
        schema::action(
            "source",
            "Read one declared UTF-8 source file",
            vec![text("id"), text("path")],
        ),
        schema::action("state", "Read the shared live state", vec![text("id")]),
        schema::action(
            "settings_names",
            "Read private setting names and revision only",
            vec![text("id")],
        ),
    ]
}
fn write_actions() -> Vec<schema::Action> {
    vec![
        schema::action(
            "create",
            "Create a project with UTF-8 files",
            vec![object("definition"), object("files")],
        ),
        schema::action(
            "save",
            "Save a definition without executing it",
            vec![object("definition"), source_revision()],
        ),
        schema::action(
            "edit_file",
            "Replace a declared UTF-8 file without executing it",
            vec![text("id"), text("path"), text("source"), source_revision()],
        ),
        schema::action(
            "apply",
            "Apply and restart the exact working revision",
            vec![text("id"), source_revision()],
        ),
        schema::action(
            "start",
            "Start the exact working revision",
            vec![text("id"), source_revision()],
        ),
        schema::action("stop", "Stop the project", vec![text("id")]),
        schema::action("close", "Stop and close the project", vec![text("id")]),
        schema::action(
            "set_value",
            "Set a declared input on the expected session",
            vec![
                text("id"),
                text("key"),
                (
                    "value",
                    json!({"anyOf":[{"type":"string"},{"type":"number"}]}),
                    true,
                ),
                input_revision(),
            ],
        ),
        schema::action(
            "invoke",
            "Invoke a declared action on the expected session",
            vec![text("id"), text("name"), input_revision()],
        ),
        schema::action(
            "export",
            "Export to the fixed project exports directory",
            vec![text("id"), source_revision()],
        ),
    ]
}
pub(super) fn spec(write: bool) -> ToolSpec {
    let (name, title, access, approval, actions) = if write {
        (
            "space_lab_project_edit",
            "Edit Space Lab projects",
            Access::Write,
            ApprovalPolicy::PerCall,
            write_actions(),
        )
    } else {
        (
            "space_lab_project_inspect",
            "Inspect Space Lab projects",
            Access::Read,
            ApprovalPolicy::None,
            read_actions(),
        )
    };
    ToolSpec {
        name,
        title,
        description: "Structured native project operations. Never pass credentials; there is no shell or arbitrary export path.",
        access,
        approval,
        arguments: schema::flat(&actions),
    }
}
