//! Pure Agent Client Protocol (v1) messages: JSON-RPC 2.0 requests Subspace
//! sends, and the answers, requests and notifications an agent sends back.
use super::{ModelChoice, SlashCommand, Usage, Value, json};
use crate::runtime::transcript;

/// The ACP major version Subspace speaks.
pub(super) const PROTOCOL_VERSION: u64 = 1;
/// JSON-RPC's code for a method the receiver does not implement.
const METHOD_NOT_FOUND: i64 = -32601;
/// The widest tool title an approval card repeats.
const SUMMARY_TITLE_LIMIT: usize = 120;
/// ACP's code for a request that needs `authenticate` first.
const AUTH_REQUIRED: i64 = -32000;

/// One line an agent wrote, by its JSON-RPC shape.
#[derive(Debug, PartialEq)]
pub(super) enum Message<'a> {
    Response {
        id: u64,
        result: Result<&'a Value, RpcError>,
    },
    Request {
        id: &'a Value,
        method: &'a str,
        params: &'a Value,
    },
    Notification {
        method: &'a str,
        params: &'a Value,
    },
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RpcError {
    pub(super) code: i64,
    pub(super) message: String,
}

impl RpcError {
    pub(super) fn needs_sign_in(&self) -> bool {
        self.code == AUTH_REQUIRED
    }
}

pub(super) fn classify(record: &Value) -> Message<'_> {
    let method = record["method"].as_str();
    let id = record.get("id").filter(|id| !id.is_null());
    match (method, id) {
        (Some(method), Some(id)) => Message::Request {
            id,
            method,
            params: &record["params"],
        },
        (Some(method), None) => Message::Notification {
            method,
            params: &record["params"],
        },
        (None, Some(id)) => id
            .as_u64()
            .map_or(Message::Invalid, |id| Message::Response {
                id,
                result: response_result(record),
            }),
        (None, None) => Message::Invalid,
    }
}

fn response_result(record: &Value) -> Result<&Value, RpcError> {
    let Some(error) = record.get("error") else {
        return Ok(&record["result"]);
    };
    let detail = error["data"]["message"]
        .as_str()
        .or_else(|| error["data"].as_str())
        .filter(|detail| !detail.trim().is_empty());
    let message = error["message"].as_str().unwrap_or("no details");
    Err(RpcError {
        code: error["code"].as_i64().unwrap_or_default(),
        message: match detail {
            Some(detail) if detail != message => format!("{message}: {detail}"),
            _ => message.to_owned(),
        },
    })
}

pub(super) fn request(id: u64, method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
}

pub(super) fn notification(method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","method":method,"params":params})
}

/// Subspace offers no file system or terminal: the agent uses its own tools.
pub(super) fn initialize_params() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "clientCapabilities": {
            "fs": {"readTextFile": false, "writeTextFile": false},
            "terminal": false
        },
        "clientInfo": {"name": "subspace", "title": "Subspace", "version": env!("CARGO_PKG_VERSION")}
    })
}

pub(super) fn reply(id: &Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":result})
}

pub(super) fn method_not_found(id: &Value, method: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": METHOD_NOT_FOUND, "message": format!("Subspace does not offer {method}.")}
    })
}

/// What the agent's `initialize` answer allows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Capabilities {
    pub(super) load_session: bool,
    pub(super) images: bool,
    /// Whether session requests may carry a `systemPrompt` to append.
    pub(super) system_prompt: bool,
    /// Whether the agent connects to Streamable HTTP MCP servers.
    pub(super) mcp_http: bool,
    /// The first advertised sign-in method, which `authenticate` names.
    pub(super) auth_method: Option<String>,
    /// How to sign in, from that method's description.
    pub(super) sign_in: Option<String>,
}

pub(super) fn capabilities(result: &Value) -> Result<Capabilities, String> {
    let version = result["protocolVersion"].as_u64();
    if version != Some(PROTOCOL_VERSION) {
        return Err(format!(
            "The agent speaks ACP version {}; Subspace supports version {PROTOCOL_VERSION}.",
            version.map_or_else(|| "unknown".to_owned(), |version| version.to_string())
        ));
    }
    let agent = &result["agentCapabilities"];
    let method = result["authMethods"]
        .as_array()
        .and_then(|methods| methods.first());
    Ok(Capabilities {
        load_session: agent["loadSession"].as_bool().unwrap_or(false),
        images: agent["promptCapabilities"]["image"]
            .as_bool()
            .unwrap_or(false),
        system_prompt: agent["sessionCapabilities"]["systemPrompt"].is_object(),
        mcp_http: agent["mcpCapabilities"]["http"].as_bool().unwrap_or(false),
        auth_method: method
            .and_then(|method| method["id"].as_str())
            .map(str::to_owned),
        sign_in: method
            .and_then(|method| {
                method["description"]
                    .as_str()
                    .or_else(|| method["name"].as_str())
            })
            .map(str::to_owned),
    })
}

/// The permission option for Subspace's yes or no: a one-time choice when the
/// agent offers one, else the lasting one. `None` answers `cancelled`.
pub(super) fn permission_option(options: &Value, allow: bool) -> Option<String> {
    let kinds = if allow {
        ["allow_once", "allow_always"]
    } else {
        ["reject_once", "reject_always"]
    };
    let options = options.as_array()?;
    kinds.iter().find_map(|kind| {
        options
            .iter()
            .find(|option| option["kind"].as_str() == Some(kind))
            .and_then(|option| option["optionId"].as_str())
            .map(str::to_owned)
    })
}

pub(super) fn permission_outcome(option: Option<&str>) -> Value {
    match option {
        Some(option) => json!({"outcome": {"outcome": "selected", "optionId": option}}),
        None => json!({"outcome": {"outcome": "cancelled"}}),
    }
}

/// The approval card's text for one `session/request_permission`.
pub(super) fn permission_summary(agent: &str, params: &Value) -> String {
    let call = &params["toolCall"];
    let title = call["title"]
        .as_str()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or("use a tool");
    let detail = transcript::argument_summary(&call["rawInput"]);
    let title = transcript::one_line(title, SUMMARY_TITLE_LIMIT);
    if detail.is_empty() || crate::repeats(&title, &detail) {
        format!("{agent} asks to {title}")
    } else {
        format!("{agent} asks to {title} ({detail})")
    }
}

/// The session's model picker, from its `select` option in the `model`
/// category, and the reasoning levels of its `thought_level` option.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ModelOptions {
    pub(super) model_option: Option<String>,
    pub(super) current: Option<String>,
    pub(super) choices: Vec<ModelChoice>,
    pub(super) effort_option: Option<String>,
    pub(super) effort: Option<String>,
}

pub(super) fn model_options(config: &Value) -> ModelOptions {
    let options = config.as_array().map(Vec::as_slice).unwrap_or_default();
    let model = selector(options, "model");
    let effort = selector(options, "thought_level");
    let efforts = effort.map(select_values).unwrap_or_default();
    let choices = model
        .map(select_values)
        .unwrap_or_default()
        .into_iter()
        .map(|(id, label)| ModelChoice {
            id,
            label,
            efforts: efforts.iter().map(|(value, _)| value.clone()).collect(),
            default_effort: None,
        })
        .collect();
    ModelOptions {
        model_option: model.and_then(option_id),
        current: model.and_then(current_value),
        choices,
        effort_option: effort.and_then(option_id),
        effort: effort.and_then(current_value),
    }
}

fn selector<'a>(options: &'a [Value], category: &str) -> Option<&'a Value> {
    options.iter().find(|option| {
        option["type"].as_str() == Some("select")
            && (option["category"].as_str() == Some(category)
                || option["id"].as_str() == Some(category))
    })
}

fn option_id(option: &Value) -> Option<String> {
    option["id"].as_str().map(str::to_owned)
}

fn current_value(option: &Value) -> Option<String> {
    option["currentValue"].as_str().map(str::to_owned)
}

/// Every `(value, name)` of a select option, flattening its groups.
fn select_values(option: &Value) -> Vec<(String, String)> {
    let entries = option["options"].as_array().into_iter().flatten();
    entries
        .flat_map(|entry| match entry["options"].as_array() {
            Some(group) => group.iter().collect::<Vec<_>>(),
            None => vec![entry],
        })
        .filter_map(|entry| {
            let value = entry["value"].as_str()?;
            let name = entry["name"].as_str().unwrap_or(value);
            Some((value.to_owned(), name.to_owned()))
        })
        .collect()
}

pub(super) fn commands(update: &Value) -> Vec<SlashCommand> {
    update["availableCommands"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|command| {
            Some(SlashCommand {
                name: command["name"].as_str()?.to_owned(),
                description: command["description"].as_str().unwrap_or("").to_owned(),
                aliases: Vec::new(),
            })
        })
        .collect()
}

pub(super) fn usage(result: &Value) -> Usage {
    let usage = &result["usage"];
    Usage {
        input_tokens: usage["inputTokens"].as_u64(),
        output_tokens: usage["outputTokens"].as_u64(),
        nano_aiu: None,
    }
}

/// What a finished turn says when it did not simply end.
pub(super) fn stop_problem(result: &Value) -> Option<&'static str> {
    match result["stopReason"].as_str() {
        Some("max_tokens") => Some("The agent stopped: it reached its token limit."),
        Some("max_turn_requests") => Some("The agent stopped: it reached its request limit."),
        Some("refusal") => Some("The agent declined to continue."),
        _ => None,
    }
}

pub(super) fn cancelled(result: &Value) -> bool {
    result["stopReason"].as_str() == Some("cancelled")
}
