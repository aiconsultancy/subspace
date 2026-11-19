use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::Parser;
use crate::config::{AcpAgent, AcpConfig, show};

/// `chat.acp`: an unusable value is reported and the preset is kept.
pub(super) fn parse(parser: &mut Parser<'_>, chat: Option<&Map<String, Value>>) -> AcpConfig {
    let section = chat.and_then(|chat| parser.section(chat, "acp"));
    AcpConfig {
        agent: agent(parser, section),
        command: parser.string(section, "chat.acp", "command"),
        args: args(parser, section.and_then(|acp| acp.get("args"))),
        env: env(parser, section.and_then(|acp| acp.get("env"))),
    }
}

fn agent(parser: &mut Parser<'_>, section: Option<&Map<String, Value>>) -> AcpAgent {
    let Some(value) = section.and_then(|acp| acp.get("agent")) else {
        return AcpAgent::default();
    };
    if let Some(agent) = value.as_str().and_then(AcpAgent::from_id) {
        return agent;
    }
    if !value.is_null() {
        parser.warn(
            "chat.acp.agent",
            format!(
                "expected opencode, claude, codex, gemini or custom, got {}; using opencode",
                show(value)
            ),
        );
    }
    AcpAgent::default()
}

fn args(parser: &mut Parser<'_>, value: Option<&Value>) -> Option<Vec<String>> {
    const PROBLEM: &str = "expected an array of strings; using the preset's";
    let Some(Value::Array(items)) = value else {
        if value.is_some_and(|value| !value.is_null()) {
            parser.warn("chat.acp.args", PROBLEM);
        }
        return None;
    };
    let args: Option<Vec<String>> = items
        .iter()
        .map(|item| item.as_str().map(str::to_owned))
        .collect();
    if args.is_none() {
        parser.warn("chat.acp.args", PROBLEM);
    }
    args
}

fn env(parser: &mut Parser<'_>, value: Option<&Value>) -> BTreeMap<String, String> {
    let Some(Value::Object(entries)) = value else {
        if value.is_some_and(|value| !value.is_null()) {
            parser.warn("chat.acp.env", "expected an object; ignored");
        }
        return BTreeMap::new();
    };
    let mut env = BTreeMap::new();
    for (name, value) in entries {
        match value.as_str() {
            Some(text) if valid_name(name) => {
                env.insert(name.clone(), text.to_owned());
            }
            _ => parser.warn(
                format!("chat.acp.env.{name}"),
                "expected a string for a NAME made of letters, digits and _; ignored",
            ),
        }
    }
    env
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(|first: char| first.is_ascii_digit())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::config::{AcpAgent, ChatEngine, parse};

    fn parsed(text: &str) -> (crate::config::Config, Vec<String>) {
        let (config, notices) = parse(text, Path::new("/c/config.json"));
        (config, notices.iter().map(ToString::to_string).collect())
    }

    #[test]
    fn acp_defaults_to_the_opencode_preset_without_overrides() {
        let (config, notices) = parsed(r#"{"chat":{"engine":"acp"}}"#);
        assert!(notices.is_empty(), "{notices:?}");
        assert_eq!(config.chat.engine, ChatEngine::Acp);
        assert_eq!(config.chat.acp.agent, AcpAgent::OpenCode);
        assert_eq!(config.chat.acp.command, None);
        assert_eq!(config.chat.acp.args, None);
        assert!(config.chat.acp.env.is_empty());
    }

    #[test]
    fn a_custom_agent_reads_its_command_arguments_and_environment() {
        let (config, notices) = parsed(
            r#"{"chat":{"engine":"acp","acp":{"agent":"custom","command":" my-agent ","args":["--acp","-v"],"env":{"AGENT_MODE":"quiet"}}}}"#,
        );
        assert!(notices.is_empty(), "{notices:?}");
        let acp = config.chat.acp;
        assert_eq!(acp.agent, AcpAgent::Custom);
        assert_eq!(acp.command.as_deref(), Some("my-agent"));
        assert_eq!(acp.args, Some(vec!["--acp".to_owned(), "-v".to_owned()]));
        assert_eq!(acp.env.get("AGENT_MODE").map(String::as_str), Some("quiet"));
    }

    #[test]
    fn unusable_acp_values_warn_and_keep_the_preset() {
        let (config, notices) = parsed(
            r#"{"chat":{"acp":{"agent":"cursor","args":["ok",7],"env":{"bad name":"x","1X":"y","GOOD":"z","N":3}}}}"#,
        );
        assert_eq!(config.chat.acp.agent, AcpAgent::OpenCode);
        assert_eq!(config.chat.acp.args, None);
        assert_eq!(config.chat.acp.env.len(), 1);
        assert_eq!(notices.len(), 5, "{notices:?}");
        assert!(notices[0].contains("chat.acp.agent"), "{notices:?}");
        assert!(notices[1].contains("chat.acp.args"), "{notices:?}");
        let (config, notices) = parsed(r#"{"chat":{"acp":{"args":"--acp","env":[]}}}"#);
        assert_eq!(config.chat.acp.args, None);
        assert_eq!(notices.len(), 2, "{notices:?}");
    }
}
