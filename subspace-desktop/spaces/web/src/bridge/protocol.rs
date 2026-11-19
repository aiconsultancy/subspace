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
    #[serde(rename = "theme")]
    Theme,
    #[serde(rename = "secrets.get")]
    Secrets,
    #[serde(rename = "secrets.set")]
    SecretsSet(super::secrets::SecretWrite),
    #[serde(rename = "issues.scope")]
    IssueScope,
    #[serde(rename = "issues.resolve")]
    IssueResolve(Vec<String>),
    #[serde(rename = "issues.open")]
    IssueOpen(String),
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

/// An event for the page's current document, dispatched by the bridge's
/// `__subspaceEvent` only while that document still has the app's origin.
pub(crate) fn event(name: &str, detail: &Value, origin: &str) -> String {
    let (name, origin) = (json!(name), json!(origin));
    format!("if(location.origin === {origin}) window.__subspaceEvent?.({name}, {detail});")
}

#[cfg(test)]
mod tests {
    use super::{event, parse, reply};
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
        for operation in [
            json!({"method": "theme"}),
            json!({"method": "secrets.get"}),
            json!({"method": "secrets.set", "value": {"key": "fleet-a", "value": "t"}}),
            json!({"method": "secrets.set", "value": {"key": "fleet-a", "value": null}}),
            json!({"method": "issues.scope"}),
            json!({"method": "issues.resolve", "value": ["ABC-1"]}),
            json!({"method": "issues.open", "value": "ABC-1"}),
        ] {
            assert!(
                parse(&message("ab", operation.clone())).is_some(),
                "{operation}"
            );
        }
        assert!(
            parse(&message(
                "ab",
                json!({"method": "issues.resolve", "value": "ABC-1"})
            ))
            .is_none()
        );
        assert!(parse(&message("ab", json!({"method": "issues.open", "value": 1}))).is_none());
        let extra = json!({"method": "secrets.set", "value": {"key": "a", "value": "t", "x": 1}});
        assert!(parse(&message("ab", extra)).is_none());
        assert!(parse(&"a".repeat(16_385)).is_none());
        let request = parse(&valid).unwrap();
        let reply = reply(&request, Ok(json!({"x": "\";evil()"})), "subspace://notes");
        assert!(reply.contains("location.origin === \"subspace://notes\""));
        assert!(reply.contains("\\\";evil()"));
    }

    #[test]
    fn events_are_origin_checked_and_their_detail_is_data() {
        let script = event("theme", &json!({"name": "\";evil()"}), "subspace://notes");
        assert!(script.starts_with("if(location.origin === \"subspace://notes\")"));
        assert!(script.contains("__subspaceEvent?.(\"theme\", {\"name\":\"\\\";evil()\"})"));
    }
}
