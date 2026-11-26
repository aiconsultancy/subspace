use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};
use subspace_agent_tools::{AgentTool, Arguments, Catalog};
use subspace_mcp::{GroupKind, GroupedCatalog};
use subspace_product::{CommandMount, ProductContext};

use crate::collision;

/// The served tools, and which of them a declared Space's board group
/// provides rather than the Application group.
#[derive(Default)]
pub(crate) struct Capabilities {
    pub catalog: Catalog,
    pub space_tools: HashSet<&'static str>,
}

impl Capabilities {
    pub fn new(context: &ProductContext, grouped: &GroupedCatalog) -> Self {
        let spaces = &context.product.boards;
        Self {
            catalog: grouped.served_catalog().tools().clone(),
            space_tools: grouped
                .groups()
                .iter()
                .filter(|group| {
                    group.kind == GroupKind::Board
                        && group.served
                        && spaces.iter().any(|space| space.id.as_str() == group.id)
                })
                .flat_map(|group| group.tools.iter().map(|tool| tool.name))
                .collect(),
        }
    }
}

pub(crate) struct Route {
    pub path: Vec<String>,
    pub tool: Arc<dyn AgentTool>,
    pub schema: Value,
    pub action: Option<String>,
    pub batched: bool,
    root_mounted: bool,
}

impl Route {
    /// The schema of a field this route's command declares as a flag. An
    /// action route supplies `action` itself, so it declares no such flag.
    pub fn field(&self, name: &str) -> Option<&Value> {
        if self.action.is_some() && name == "action" {
            return None;
        }
        self.schema["properties"].get(name)
    }
}

pub(crate) struct Commands {
    pub spec: usage::Spec,
    pub routes: Vec<Route>,
}

impl Commands {
    pub fn new(
        context: &ProductContext,
        capabilities: &Capabilities,
        tree: fn(&ProductContext) -> Result<usage::Spec, String>,
        specialized_tools: &[&str],
    ) -> Result<Self, String> {
        let routes = routes(context, capabilities, specialized_tools);
        collision::validate_names(&routes)?;
        let mut spec = tree(context)?;
        let generated = usage_kdl(&routes);
        let extra = generated
            .parse::<usage::Spec>()
            .map_err(|error| error.to_string())?;
        collision::reject_host_words(&extra, context.product.update.is_some())?;
        collision::reject_flag_overlap(&spec, &extra)?;
        collision::merge_spec(&mut spec, extra)?;
        Ok(Self { spec, routes })
    }
}

fn routes(
    context: &ProductContext,
    capabilities: &Capabilities,
    specialized_tools: &[&str],
) -> Vec<Route> {
    let mut routes: Vec<Route> = capabilities
        .catalog
        .tools()
        .iter()
        .filter(|tool| !specialized_tools.contains(&tool.spec().name))
        .flat_map(|tool| tool_routes(context, &capabilities.space_tools, tool))
        .collect();
    if context.product.command_mount == CommandMount::Prefixed {
        disambiguate_single_roots(&mut routes);
    }
    routes
}

fn disambiguate_single_roots(routes: &mut [Route]) {
    let mut counts = BTreeMap::new();
    for route in routes.iter() {
        *counts.entry(route.path[0].clone()).or_insert(0usize) += 1;
    }
    for route in routes.iter_mut() {
        if route.action.is_none() && counts[&route.path[0]] > 1 {
            route.path[0] = route.tool.spec().name.replace('_', "-");
        }
    }
}

/// The word a tool's commands group under, and whether they mount at the
/// binary's root instead. Root mode flattens only the tools the one Space
/// provides; Prefixed mode groups a tool under the Space its name starts with.
fn mount<'a>(
    context: &'a ProductContext,
    space_tools: &HashSet<&str>,
    name: &'a str,
) -> (&'a str, bool) {
    let spaces = &context.product.boards;
    match (context.product.command_mount, spaces.first()) {
        (CommandMount::Root, Some(space)) if space_tools.contains(name) => {
            (space.id.as_str(), true)
        }
        (CommandMount::Root, _) => (name, false),
        (CommandMount::Prefixed, _) => (
            spaces
                .iter()
                .filter(|space| {
                    name.starts_with(&format!("{}_", space.id.as_str().replace('-', "_")))
                })
                .max_by_key(|space| space.id.as_str().len())
                .map_or(name, |space| space.id.as_str()),
            false,
        ),
    }
}

fn tool_routes(
    context: &ProductContext,
    space_tools: &HashSet<&str>,
    tool: &Arc<dyn AgentTool>,
) -> Vec<Route> {
    let (root, mounted_at_root) = mount(context, space_tools, tool.spec().name);
    match &tool.spec().arguments {
        Arguments::Actions { actions, batched } => actions
            .iter()
            .zip(tool.spec().arguments.action_names())
            .map(|(action, name)| Route {
                path: action_path(root, name, mounted_at_root),
                tool: tool.clone(),
                schema: subspace_agent_tools::schema::flat(std::slice::from_ref(action))
                    .input_schema(),
                action: Some(name.into()),
                batched: *batched,
                root_mounted: mounted_at_root,
            })
            .collect(),
        Arguments::Single { .. } => vec![Route {
            path: vec![if mounted_at_root {
                action_name(root, tool.spec().name)
            } else {
                root.into()
            }],
            tool: tool.clone(),
            schema: tool.spec().input_schema(),
            action: None,
            batched: false,
            root_mounted: mounted_at_root,
        }],
    }
}

fn action_path(root: &str, name: &str, mounted_at_root: bool) -> Vec<String> {
    let command = action_name(root, name);
    if mounted_at_root {
        vec![command]
    } else {
        vec![root.into(), command]
    }
}

fn usage_kdl(routes: &[Route]) -> String {
    let mut groups: BTreeMap<&str, Vec<&Route>> = BTreeMap::new();
    for route in routes {
        groups.entry(&route.path[0]).or_default().push(route);
    }
    groups
        .into_iter()
        .map(|(root, routes)| {
            if routes[0].path.len() == 1 {
                return command_kdl(root, routes[0]);
            }
            let children = routes
                .iter()
                .map(|route| command_kdl(&route.path[1], route))
                .collect::<Vec<_>>()
                .join("\n");
            format!("cmd {root:?} help={root:?} {{\n{children}\n}}\n")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn action_name(board: &str, action: &str) -> String {
    action
        .strip_prefix(&format!("{board}_"))
        .or_else(|| action.strip_prefix(&format!("{}_", board.replace('-', "_"))))
        .unwrap_or(action)
        .replace('_', "-")
}

pub(crate) fn single_alias(route: &Route) -> Option<&str> {
    let name = route.tool.spec().name;
    (route.action.is_none()
        && route.path[0] != name
        && (route.root_mounted || route.path[0] == name.replace('_', "-")))
    .then_some(name)
}

fn description(route: &Route) -> &str {
    route.schema["properties"]["action"]["description"]
        .as_str()
        .and_then(|guide| guide.split_once(": "))
        .map(|(_, purpose)| purpose)
        .unwrap_or(route.tool.spec().description)
}

fn command_kdl(name: &str, route: &Route) -> String {
    let description = description(route);
    let mut flags = vec![
        "flag \"--json\" help=\"Print the exact tool JSON\"".to_string(),
        "flag \"--yes\" help=\"Approve this call after validation and preflight\"".into(),
    ];
    if route.batched {
        flags.push("flag \"--operations-file <PATH>\" help=\"JSON array of 1 to 20 operations (maximum 1 MiB)\"".into());
    }
    if let Some(action) = &route.action
        && action != name
    {
        flags.push(format!("alias {action:?} hide=#true"));
    }
    if let Some(alias) = single_alias(route) {
        flags.push(format!("alias {alias:?} hide=#true"));
    }
    if let Some(properties) = route.schema["properties"].as_object() {
        flags.extend(
            properties
                .iter()
                .filter(|(name, _)| route.field(name).is_some())
                .map(|(name, schema)| field_kdl(name, schema)),
        );
    }
    format!(
        "cmd {name:?} help={description:?} {{\n{}\n}}\n",
        flags.join("\n")
    )
}

fn field_kdl(name: &str, schema: &Value) -> String {
    let flag = format!("--{name} <VALUE>");
    let help = schema["description"]
        .as_str()
        .unwrap_or("Capability field; objects and arrays use JSON");
    let choices = schema["enum"].as_array().map(|values| {
        values
            .iter()
            .filter_map(Value::as_str)
            .map(|value| format!("{value:?}"))
            .collect::<Vec<_>>()
            .join(" ")
    });
    match choices.filter(|choices| !choices.is_empty()) {
        Some(choices) => format!("flag {flag:?} help={help:?} {{\nchoices {choices}\n}}"),
        None => format!("flag {flag:?} help={help:?}"),
    }
}
