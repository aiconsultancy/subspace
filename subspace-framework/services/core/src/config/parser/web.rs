use serde_json::{Map, Value};

use super::Parser;
use crate::config::{WebConfig, WebSearchConfig};
use crate::http::plain_https_url;

pub(super) fn parse(parser: &mut Parser<'_>, section: Option<&Map<String, Value>>) -> WebConfig {
    let search = section.and_then(|web| parser.section(web, "search"));
    WebConfig {
        allowed_hosts: hosts(
            parser,
            section.and_then(|web| web.get("allowedHosts")),
            "web.allowedHosts",
        ),
        silent_allowed_hosts: hosts(
            parser,
            section.and_then(|web| web.get("silentAllowedHosts")),
            "web.silentAllowedHosts",
        ),
        search: WebSearchConfig {
            provider: parser.string(search, "web.search", "provider"),
            endpoint: endpoint(parser, search),
        },
    }
}

fn endpoint(parser: &mut Parser<'_>, section: Option<&Map<String, Value>>) -> Option<String> {
    parser
        .string(section, "web.search", "endpoint")
        .and_then(|value| {
            plain_https_url(&value)
                .map_err(|error| parser.warn("web.search.endpoint", error.to_string()))
                .ok()
        })
}

fn hosts(parser: &mut Parser<'_>, value: Option<&Value>, key: &str) -> Vec<String> {
    let entries = parser.array(value, key).to_vec();
    let mut hosts = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let Some(host) = entry
            .as_str()
            .map(str::trim)
            .filter(|host| valid_rule(host))
        else {
            parser.warn(
                format!("{key}[{index}]"),
                "expected an exact host or one leading *. label; ignored",
            );
            continue;
        };
        let host = host.to_ascii_lowercase();
        if !hosts.contains(&host) {
            hosts.push(host);
        }
    }
    hosts
}

fn valid_rule(value: &str) -> bool {
    let host = value.strip_prefix("*.").unwrap_or(value);
    !host.is_empty()
        && !host.starts_with('.')
        && !host.ends_with('.')
        && !host.contains("..")
        && host.split('.').all(|label| {
            !label.is_empty()
                && label
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
        })
}
