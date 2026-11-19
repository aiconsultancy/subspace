//! Resource descriptors and URI routing, independent of transport and hosting.
use rmcp::model::{Resource, ResourceTemplate};
use serde_json::Value;

pub(crate) type ResourceDescriptor = (&'static str, &'static str, &'static str);
pub(crate) type ResourceTemplateDescriptor =
    (&'static str, &'static str, &'static str, &'static str);

pub(crate) fn resource((uri, name, title): ResourceDescriptor) -> Resource {
    Resource::new(uri, name)
        .with_title(title)
        .with_mime_type("application/json")
}

pub(crate) fn resource_template(
    (uri_template, name, title, description): ResourceTemplateDescriptor,
) -> ResourceTemplate {
    ResourceTemplate::new(uri_template, name)
        .with_title(title)
        .with_description(description)
        .with_mime_type("application/json")
}

pub(crate) fn resource_descriptors(help: bool) -> Vec<ResourceDescriptor> {
    let mut resources = Vec::new();
    if help {
        resources.push(("subspace://help", "help", "Active board help"));
    }
    resources
}

pub(crate) fn resource_template_descriptors(
    search: bool,
    cache: bool,
) -> Vec<ResourceTemplateDescriptor> {
    let mut templates = Vec::new();
    if search {
        templates.push((
            "subspace://search{?q,boards}",
            "search",
            "Cached search",
            "Search cached board content with query and board filters.",
        ));
    }
    if cache {
        templates.push((
            "subspace://cache/{board}/{account}/{id}",
            "cached-document",
            "Cached document",
            "Read a cached search document by board, account and result id.",
        ));
    }
    templates
}

pub(crate) fn resource_call(uri: &str) -> Option<(&'static str, Value)> {
    if uri == "subspace://help" {
        return Some((
            subspace_agent_tools::HELP_TOOL,
            serde_json::json!({"action":"list"}),
        ));
    }
    let query = uri.strip_prefix("subspace://search?q=")?;
    let (query, boards) = query.split_once("&boards=")?;
    let boards: Vec<String> = boards
        .split(',')
        .filter(|board| !board.is_empty())
        .map(str::to_owned)
        .collect();
    Some((
        "search_content",
        serde_json::json!({"query":query,"boards":boards}),
    ))
}
