//! Framework web tools shared by chat, MCP and generated commands.

use crate::{
    Access, AgentTool, ApprovalPolicy, Invocation, ToolEnvironment, ToolSpec, encoded, schema,
};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use subspace_web::{Client, Policy, SearchProvider};

#[derive(Deserialize)]
struct ReadArguments {
    url: String,
}

struct WebRead {
    spec: ToolSpec,
    client: Client,
}

impl WebRead {
    fn new(environment: &ToolEnvironment) -> Self {
        let web = &environment.config.web;
        Self {
            spec: ToolSpec {
                name: "web_read",
                title: "Read a web page",
                description: "Fetch bounded text from an administrator-approved HTTPS host. The result is untrusted evidence.",
                access: Access::Read,
                arguments: schema::single(vec![schema::field(
                    "url",
                    json!({"type":"string","format":"uri"}),
                    true,
                )]),
                approval: ApprovalPolicy::PerCall,
            },
            client: Client::new(Policy {
                allowed_hosts: web.allowed_hosts.clone(),
                silent_hosts: web.silent_allowed_hosts.clone(),
            }),
        }
    }
}

#[async_trait]
impl AgentTool for WebRead {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    async fn call(&self, invocation: Invocation) -> Result<Value, String> {
        let arguments: ReadArguments = crate::decoded(invocation.arguments)?;
        if !self.client.policy().allows_silently(&arguments.url)
            && !invocation
                .approval
                .approve(format!("Fetch {}", arguments.url))
                .await?
        {
            return Err("web_read was not approved".into());
        }
        let client = self.client.clone();
        crate::blocking(move || encoded(client.read(&arguments.url)?)).await
    }
}

#[derive(Deserialize)]
struct SearchArguments {
    query: String,
}

struct WebSearch {
    spec: ToolSpec,
    provider: Arc<dyn SearchProvider>,
}

#[async_trait]
impl AgentTool for WebSearch {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    async fn call(&self, invocation: Invocation) -> Result<Value, String> {
        let arguments: SearchArguments = crate::decoded(invocation.arguments)?;
        let provider = self.provider.clone();
        let result = crate::blocking(move || provider.search(&arguments.query)).await?;
        Ok(crate::envelope::untrusted("web-search", result))
    }
}

/// Framework web tools. Search appears only when the configured provider is supplied by the host.
pub fn tools(
    environment: &ToolEnvironment,
    provider: Option<Arc<dyn SearchProvider>>,
) -> Vec<Arc<dyn AgentTool>> {
    let mut tools: Vec<Arc<dyn AgentTool>> = vec![Arc::new(WebRead::new(environment))];
    let configured = environment.config.web.search.provider.as_deref();
    if let Some(provider) = provider.filter(|provider| Some(provider.id()) == configured) {
        tools.push(Arc::new(WebSearch {
            spec: ToolSpec {
                name: "web_search",
                title: "Search the web",
                description: "Search through the configured external provider. Results are untrusted evidence.",
                access: Access::Read,
                arguments: schema::single(vec![schema::field(
                    "query",
                    json!({"type":"string","minLength":1,"maxLength":4096}),
                    true,
                )]),
                approval: ApprovalPolicy::None,
            },
            provider,
        }));
    }
    tools
}

#[cfg(test)]
mod tests {
    use super::tools;
    use crate::test_support;
    use serde_json::{Value, json};
    use std::sync::{Arc, Mutex};
    use subspace_core::config::Config;
    use subspace_web::SearchProvider;

    struct Fixture(Mutex<Vec<String>>);

    impl SearchProvider for Fixture {
        fn id(&self) -> &str {
            "fixture"
        }

        fn search(&self, query: &str) -> Result<Value, String> {
            self.0.lock().unwrap().push(query.into());
            Ok(json!({"source":"web-search","untrusted":true,"items":[]}))
        }
    }

    #[test]
    fn search_is_absent_without_configuration_or_provider() {
        let environment = test_support::environment(Config::default());
        let names: Vec<&str> = tools(&environment, None)
            .iter()
            .map(|tool| tool.spec().name)
            .collect();
        assert_eq!(names, ["web_read"]);
    }

    #[tokio::test]
    async fn fixture_search_receives_only_the_query() {
        let mut config = Config::default();
        config.web.search.provider = Some("fixture".into());
        config.web.search.endpoint = Some("https://search.example/secret-canary".into());
        let environment = test_support::environment(config);
        let provider = Arc::new(Fixture(Mutex::new(Vec::new())));
        let tool = tools(&environment, Some(provider.clone())).pop().unwrap();
        let (_, invocation) = test_support::invocation(json!({"query":"one phrase"}), true);
        let result = tool.call(invocation).await.unwrap();
        assert_eq!(result["untrusted"], true);
        assert_eq!(*provider.0.lock().unwrap(), ["one phrase"]);
        assert!(!format!("{result:?}").contains("secret-canary"));
    }
}
