//! One read tool. The same declaration serves chat, the MCP server and the
//! generated command `harbor hello greet --name <NAME>`.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};
use subspace_framework::agent_tools::schema::{action, flat, text};
use subspace_framework::agent_tools::{Access, AgentTool, ApprovalPolicy, ReadService, ReadTool, ToolSpec};

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Request {
    Greet { name: String },
}

struct Greeter;

impl ReadService for Greeter {
    type Request = Request;

    fn read(&self, request: Request) -> Result<Value, String> {
        let Request::Greet { name } = request;
        Ok(json!({ "greeting": format!("Hello, {name}.") }))
    }
}

pub(crate) fn tools() -> Vec<Arc<dyn AgentTool>> {
    let spec = ToolSpec {
        name: "hello_read",
        title: "Greet someone",
        description: "Returns a greeting. It reads no service and stores nothing.",
        access: Access::Read,
        arguments: flat(&[action(
            "greet",
            "Return a greeting for name.",
            vec![("name", text(), true)],
        )]),
        approval: ApprovalPolicy::None,
    };
    vec![Arc::new(ReadTool::new(spec, Arc::new(Greeter)))]
}

#[cfg(test)]
mod tests {
    use super::{Greeter, Request};
    use subspace_framework::agent_tools::ReadService as _;

    #[test]
    fn greet_names_the_person() {
        let reply = Greeter.read(Request::Greet { name: "Ada".into() }).unwrap();
        assert_eq!(reply["greeting"], "Hello, Ada.");
    }
}
