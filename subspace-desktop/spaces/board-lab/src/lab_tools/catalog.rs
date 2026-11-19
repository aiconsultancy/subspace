use std::path::PathBuf;
use std::sync::Arc;

use subspace_framework::agent_tools::{Access, AgentTool, ApprovalPolicy, ToolSpec};

use super::edit;
use super::environment;
use super::inspect;
use super::run;
use super::schema;
use super::{Group, Handler, LabTool};

pub(super) fn tools(root: PathBuf) -> Vec<Arc<dyn AgentTool>> {
    [
        (
            Group::Inspect,
            "board_lab_inspect",
            "Inspect the managed board lab",
            "environments, presets, draft, canvas, network_policies, recipes, source, dashboard",
            inspect::run as Handler,
        ),
        (
            Group::Edit,
            "board_lab_edit",
            "Edit and save a visualization preset",
            "add_control, parameters, save, delete, import_canvas, arrange",
            edit::run as Handler,
        ),
        (
            Group::Run,
            "board_lab_run",
            "Run a sandboxed recipe or code",
            "recipe, execute, network_probe",
            run::run as Handler,
        ),
        (
            Group::Environment,
            "board_lab_environment",
            "Manage board projects and recipe source",
            "register_environment, provision, set_version, add_package, add_tool, configure_network, register_recipe",
            environment::run as Handler,
        ),
    ]
    .into_iter()
    .map(|(group, name, title, actions, handler)| {
        Arc::new(LabTool {
            group,
            root: root.clone(),
            handler,
            spec: spec(group, name, title, actions),
        }) as Arc<dyn AgentTool>
    })
    .collect()
}

fn spec(group: Group, name: &'static str, title: &'static str, actions: &'static str) -> ToolSpec {
    ToolSpec {
        name,
        title,
            description: Box::leak(format!("Legacy fixture recipes in Board Lab alpha. Actions: {actions}. Use structured arguments, not shell commands. These one-shot operations use MXC without credentials. Network is denied unless execute explicitly requests network=true, then only configured destinations via the host proxy are reachable. Provisioning is a separate trusted network-enabled step. For a new fixture draft use profile market, services or typescript; pass returned preset to subsequent calls. Saving persists the recipe, parameters, controls and optional snapshot. Dataset results are local fixture data, not EVA or live exchange quotes. Use board_lab_project_inspect and board_lab_project_edit for native persistent projects.").into_boxed_str()),
        access: if matches!(group, Group::Inspect) {
            Access::Read
        } else {
            Access::Write
        },
        arguments: schema::arguments(actions),
        approval: if matches!(group, Group::Inspect) {
            ApprovalPolicy::None
        } else {
            ApprovalPolicy::PerCall
        },
    }
}
