//! Convert SDK-parsed metadata and a validated staging file into an agent.
use super::{CustomAgentConfig, Path, PathBuf, read_agent_prompt};
use github_copilot_sdk::rpc::AgentInfo;

pub(super) fn convert_agent(
    agent: AgentInfo,
    stage: &Path,
    enable_skills: bool,
) -> Result<Option<CustomAgentConfig>, String> {
    let Some(path) = staged_agent_path(&agent, stage)? else {
        return Ok(None);
    };
    let content = read_agent_prompt(&path)?;
    let prompt = prompt_body(&content)?.to_owned();
    Ok(Some(agent_config(agent, prompt, enable_skills)))
}

fn staged_agent_path(agent: &AgentInfo, stage: &Path) -> Result<Option<PathBuf>, String> {
    if agent.name == "subspace" {
        return Ok(None);
    }
    // Exclude remote or unrelated agents the discovery service may also return.
    let Some(path) = agent.path.as_deref().map(Path::new) else {
        return Ok(None);
    };
    if path.parent() != Some(stage) {
        return Ok(None);
    }
    if agent
        .mcp_servers
        .as_ref()
        .is_some_and(|servers| !servers.is_empty())
    {
        return Err("An agent declares MCP servers. Embedded integrations are blocked; configure explicit Subspace MCP settings instead.".into());
    }
    Ok(Some(path.to_path_buf()))
}

// Discovery may omit prompt text. Use the private staging copy as its source.
fn prompt_body(content: &str) -> Result<&str, String> {
    let content = content.trim_start_matches('\u{feff}');
    let header = content
        .strip_prefix("---\n")
        .or_else(|| content.strip_prefix("---\r\n"));
    let Some(header) = header else {
        return Ok(content);
    };
    let mut offset = 0;
    for line in header.split_inclusive('\n') {
        offset += line.len();
        if matches!(line.trim_end_matches(['\r', '\n']), "---" | "...") {
            return Ok(&header[offset..]);
        }
    }
    Err("Agent prompt front matter needs a closing delimiter.".into())
}

fn agent_config(agent: AgentInfo, prompt: String, enable_skills: bool) -> CustomAgentConfig {
    let mut config = CustomAgentConfig::new(agent.name, prompt);
    config.display_name = Some(agent.display_name);
    config.description = Some(agent.description);
    config.model = agent.model;
    config.tools = agent.tools;
    config.skills = if enable_skills {
        agent.skills
    } else {
        Some(Vec::new())
    };
    config.reasoning_effort = agent.reasoning_effort;
    config
}
