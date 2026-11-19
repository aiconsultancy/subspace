use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};
use subspace_agent_tools::{AgentTool, Arguments, Catalog};
use subspace_product::ProductContext;

pub(crate) struct Route {
    pub path: Vec<String>,
    pub tool: Arc<dyn AgentTool>,
    pub schema: Value,
    pub action: Option<String>,
    pub batched: bool,
}

pub(crate) struct Commands {
    pub spec: usage::Spec,
    pub routes: Vec<Route>,
}

impl Commands {
    pub fn new(
        context: &ProductContext,
        catalog: &Catalog,
        tree: fn(&ProductContext) -> Result<usage::Spec, String>,
    ) -> Result<Self, String> {
        let routes = routes(context, catalog);
        validate_names(&routes)?;
        let mut spec = tree(context)?;
        let generated = usage_kdl(&routes);
        let extra = generated
            .parse::<usage::Spec>()
            .map_err(|error| error.to_string())?;
        for (name, command) in extra.cmd.subcommands {
            if spec.cmd.subcommands.insert(name.clone(), command).is_some() {
                return Err(format!(
                    "Generated command collides with existing command: {name}"
                ));
            }
        }
        Ok(Self { spec, routes })
    }
}

fn routes(context: &ProductContext, catalog: &Catalog) -> Vec<Route> {
    let mut routes: Vec<Route> = catalog
        .tools()
        .iter()
        .flat_map(|tool| tool_routes(context, tool))
        .collect();
    disambiguate_single_roots(&mut routes);
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

fn tool_routes(context: &ProductContext, tool: &Arc<dyn AgentTool>) -> Vec<Route> {
    let root = context
        .product
        .boards
        .iter()
        .filter(|board| {
            tool.spec()
                .name
                .starts_with(&format!("{}_", board.id.as_str().replace('-', "_")))
        })
        .max_by_key(|board| board.id.as_str().len())
        .map(|board| board.id.as_str())
        .unwrap_or(tool.spec().name);
    match &tool.spec().arguments {
        Arguments::Actions { actions, batched } => actions
            .iter()
            .zip(tool.spec().arguments.action_names())
            .map(|(action, name)| Route {
                path: vec![root.into(), action_name(root, name)],
                tool: tool.clone(),
                schema: subspace_agent_tools::schema::flat(std::slice::from_ref(action))
                    .input_schema(),
                action: Some(name.into()),
                batched: *batched,
            })
            .collect(),
        Arguments::Single { .. } => vec![Route {
            path: vec![root.into()],
            tool: tool.clone(),
            schema: tool.spec().input_schema(),
            action: None,
            batched: false,
        }],
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
            if routes[0].action.is_none() {
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

fn validate_names(routes: &[Route]) -> Result<(), String> {
    let mut names = HashSet::new();
    for route in routes {
        let canonical = route.path.join(" ");
        if route.path.iter().any(String::is_empty) || !names.insert(canonical.clone()) {
            return Err(format!("Generated command collision: {canonical}"));
        }
        if let Some(action) = &route.action
            && action != &route.path[1]
        {
            let alias = format!("{} {action}", route.path[0]);
            if !names.insert(alias.clone()) {
                return Err(format!("Generated command collision: {alias}"));
            }
        }
        if let Some(alias) = single_alias(route)
            && !names.insert(alias.to_string())
        {
            return Err(format!("Generated command collision: {alias}"));
        }
    }
    Ok(())
}

fn single_alias(route: &Route) -> Option<&str> {
    let name = route.tool.spec().name;
    (route.action.is_none() && route.path[0] != name && route.path[0] == name.replace('_', "-"))
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
                .filter(|(name, _)| route.action.is_none() || name.as_str() != "action")
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
