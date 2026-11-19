//! Bounded messages from a web document, and document-addressed replies.

use serde::Deserialize;
use serde_json::{Value, json};

pub(crate) const MAX_MESSAGE_BYTES: usize = 16 * 1024;
const MAX_DOCUMENT_CHARS: usize = 64;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub id: u32,
    pub document: String,
    pub operation: Operation,
}

#[derive(Deserialize)]
#[serde(tag = "method", content = "value", deny_unknown_fields)]
pub(crate) enum Operation {
    #[serde(rename = "info")]
    Info,
    #[serde(rename = "settings.get")]
    Settings,
    #[serde(rename = "badge.set")]
    Badge(Option<String>),
    #[serde(rename = "context.set")]
    Context(Option<Context>),
    #[serde(rename = "notify")]
    Notify(Message),
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Context {
    pub label: String,
    pub content: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Message {
    pub title: String,
    #[serde(default)]
    pub body: String,
}

pub(crate) fn parse(text: &str) -> Option<Request> {
    if text.len() > MAX_MESSAGE_BYTES {
        return None;
    }
    let request: Request = serde_json::from_str(text).ok()?;
    let document = &request.document;
    (!document.is_empty()
        && document.len() <= MAX_DOCUMENT_CHARS
        && document.bytes().all(|byte| byte.is_ascii_hexdigit()))
    .then_some(request)
}

/// The reply checks the document token before returning any data. A
/// navigation between the native check and script execution drops it.
pub(crate) fn reply(
    request: &Request,
    result: Result<Value, &'static str>,
    origin: &str,
) -> String {
    let value = match result {
        Ok(value) => json!({"id": request.id, "document": request.document, "value": value}),
        Err(error) => json!({"id": request.id, "document": request.document, "error": error}),
    };
    let origin = json!(origin);
    format!("if(location.origin === {origin}) window.__subspaceReply?.({value});")
}

#[cfg(test)]
mod tests {
    use super::{parse, reply};
    use serde_json::json;

    #[test]
    fn messages_have_a_bounded_document_token_and_a_known_operation() {
        let message = |document, operation| {
            json!({"id": 1, "document": document, "operation": operation}).to_string()
        };
        let valid = message("abcd", json!({"method": "settings.get"}));
        assert!(parse(&valid).is_some());
        assert!(parse(&message("injected()", json!({"method": "info"}))).is_none());
        assert!(parse(&message("a", json!({"method": "exec"}))).is_none());
        assert!(parse(&"a".repeat(16_385)).is_none());
        let request = parse(&valid).unwrap();
        let reply = reply(&request, Ok(json!({"x": "\";evil()"})), "subspace://notes");
        assert!(reply.contains("location.origin === \"subspace://notes\""));
        assert!(reply.contains("\\\";evil()"));
    }
}
