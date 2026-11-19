use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use subspace_search::{ReferenceIdentity, RelatedQuery, Service};

use crate::{Access, AgentTool, ApprovalPolicy, Invocation, ToolSpec, blocking, envelope, schema};

use super::{
    MAX_BOARD_BYTES, MAX_BOARDS, MAX_RESULTS, bounded_response, default_limit, selected_boards,
};

pub(super) struct RelatedTool {
    service: Service,
    boards: Vec<String>,
    spec: ToolSpec,
}

impl RelatedTool {
    pub(super) fn new(service: Service, boards: Vec<String>) -> Self {
        Self {
            service,
            boards,
            spec: ToolSpec {
                name: "related_items",
                title: "Find related indexed items",
                description: "Find retained items by an exact normalized reference. Heuristic records remain labelled with evidence and confidence. Uses only selected verified account scopes and never fetches service data.",
                access: Access::Read,
                arguments: arguments(),
                approval: ApprovalPolicy::None,
            },
        }
    }
}

fn arguments() -> crate::Arguments {
    schema::single(vec![
        schema::field("reference", json!({"type":"string","maxLength":1024}), true),
        schema::field(
            "relation_types",
            json!({"type":"array","maxItems":32,"items":{"type":"string","maxLength":128}}),
            false,
        ),
        schema::field(
            "boards",
            json!({"type":"array","maxItems":MAX_BOARDS,"items":{"type":"string","maxLength":MAX_BOARD_BYTES}}),
            false,
        ),
        schema::field(
            "limit",
            json!({"type":"integer","minimum":1,"maximum":MAX_RESULTS,"default":20}),
            false,
        ),
    ])
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RelatedArguments {
    reference: String,
    #[serde(default)]
    relation_types: Vec<String>,
    #[serde(default)]
    boards: Vec<String>,
    #[serde(default = "default_limit")]
    limit: usize,
}

#[async_trait]
impl AgentTool for RelatedTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    async fn call(&self, invocation: Invocation) -> Result<Value, String> {
        let service = self.service.clone();
        let allowed = self.boards.clone();
        blocking(move || related(&service, invocation.arguments, &allowed)).await
    }
}

fn related(service: &Service, arguments: Value, allowed: &[String]) -> Result<Value, String> {
    let arguments: RelatedArguments = serde_json::from_value(arguments)
        .map_err(|error| format!("Invalid related_items arguments: {error}"))?;
    let boards = selected_boards(service, &arguments.boards, allowed)?;
    let reference = ReferenceIdentity::parse(&arguments.reference)?;
    let items = service.related_items(
        &reference,
        &RelatedQuery {
            relation_types: arguments.relation_types,
            boards,
            limit: arguments.limit,
        },
    )?;
    let hits = items.into_iter().map(hit).collect::<Vec<_>>();
    bounded_response(envelope::untrusted(
        "cache",
        json!({"coverage":"Exact recorded references in fetched and retained content only; not complete service history.","hits":hits,"truncated":false}),
    ))
}

fn hit(item: subspace_search::RelatedItem) -> Value {
    json!({
        "board": item.hit.target.scope.board, "account": item.hit.target.scope.account,
        "id": item.hit.target.id, "title": item.hit.document.title,
        "kind": item.hit.document.kind, "parent_id": item.hit.document.parent_id,
        "refreshed_at": item.hit.document.refreshed_at, "service_url": item.hit.document.url,
        "relation_type": item.relation_type, "match": item.matched,
        "open_in_subspace": subspace_search::link(&item.hit.target),
    })
}
