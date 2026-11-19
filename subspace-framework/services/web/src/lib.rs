//! Bounded web reads and the provider boundary for optional web search.

mod fetch;
mod rate;
mod safety;
mod text;

use chrono::{SecondsFormat, Utc};
use fetch::{Backend, Response, UreqBackend};
use rate::Limits;
use serde::Serialize;
use serde_json::Value;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use url::Url;

const MAX_REDIRECTS: usize = 3;
const TOTAL_TIMEOUT: Duration = Duration::from_secs(10);

/// One optional search implementation supplied by a host outside this repository.
pub trait SearchProvider: Send + Sync {
    fn id(&self) -> &str;
    fn search(&self, query: &str) -> Result<Value, String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    pub allowed_hosts: Vec<String>,
    pub silent_hosts: Vec<String>,
}

impl Policy {
    pub fn allows_silently(&self, url: &str) -> bool {
        safety::parsed_host(url).is_ok_and(|host| safety::host_allowed(&host, &self.silent_hosts))
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WebReadResult {
    pub source: &'static str,
    pub untrusted: bool,
    pub url: String,
    pub final_url: String,
    pub fetched_at: String,
    pub content_type: String,
    pub text: String,
    pub truncated: bool,
}

#[derive(Clone)]
pub struct Client {
    policy: Policy,
    backend: Arc<dyn Backend>,
    limits: Limits,
}

impl Client {
    pub fn new(policy: Policy) -> Self {
        Self {
            policy,
            backend: Arc::new(UreqBackend),
            limits: Limits::shared(),
        }
    }

    pub fn read(&self, requested: &str) -> Result<WebReadResult, String> {
        self.limits.take()?;
        let started = Instant::now();
        let initial = safety::validate_url(requested, &self.policy.allowed_hosts)?;
        self.follow(requested, initial, started)
    }

    fn follow(
        &self,
        requested: &str,
        mut current: Url,
        started: Instant,
    ) -> Result<WebReadResult, String> {
        for redirects in 0..=MAX_REDIRECTS {
            let response = self
                .backend
                .get(&current, TOTAL_TIMEOUT.saturating_sub(started.elapsed()))?;
            if let Some(location) = response.location {
                current = self.redirect(&current, &location, redirects)?;
                continue;
            }
            return result(requested, current, response);
        }
        Err("web_read redirect handling failed".into())
    }

    fn redirect(&self, current: &Url, location: &str, count: usize) -> Result<Url, String> {
        if count == MAX_REDIRECTS {
            return Err("web_read refused more than three redirects".into());
        }
        let next = current
            .join(location)
            .map_err(|_| "web_read received an invalid redirect")?;
        safety::validate_url(next.as_str(), &self.policy.allowed_hosts)
    }

    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    #[cfg(test)]
    fn with_backend(policy: Policy, backend: impl Backend + 'static) -> Self {
        Self {
            policy,
            backend: Arc::new(backend),
            limits: Limits::isolated(),
        }
    }
}

fn result(requested: &str, current: Url, response: Response) -> Result<WebReadResult, String> {
    let content_type = text::accepted_content_type(&response.content_type)?;
    let (body, truncated) = text::bounded_text(response.body);
    Ok(WebReadResult {
        source: "web",
        untrusted: true,
        url: requested.to_owned(),
        final_url: current.into(),
        fetched_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        content_type: content_type.to_owned(),
        text: text::strip_active_content(&body, content_type),
        truncated,
    })
}

impl fmt::Debug for Client {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Client")
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
