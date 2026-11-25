use super::{
    AgentsDiscoverRequest, Client, CustomAgentConfig, MAX_AGENT_FILES, PROMPT_LIMIT, Path, PathBuf,
    config, contains_secret, fs, next_id, private_directory, state,
};
use std::io::Read;

#[path = "catalog/definition.rs"]
mod definition;
use definition::convert_agent;

#[cfg(test)]
#[path = "catalog/tests.rs"]
mod tests;

pub(super) async fn load_agents(
    client: &Client,
    settings: &config::CopilotConfig,
    workspace: &Path,
) -> Result<Vec<CustomAgentConfig>, String> {
    let catalog = stage_agent_catalog(settings, workspace)?;
    let result = discover_agents(client, settings, &catalog).await;
    // The configs carry the prompts, so the staging copy is not kept.
    let _ = fs::remove_dir_all(&catalog);
    result
}

async fn discover_agents(
    client: &Client,
    settings: &config::CopilotConfig,
    catalog: &Path,
) -> Result<Vec<CustomAgentConfig>, String> {
    let stage = catalog.join(".github/agents");
    let discovered = client
        .rpc()
        .agents()
        .discover(AgentsDiscoverRequest {
            exclude_host_agents: Some(true),
            project_paths: Some(vec![catalog.to_string_lossy().into_owned()]),
        })
        .await
        .map_err(|_| "SDK custom-agent discovery is unavailable.".to_string())?;
    let mut agents = vec![CustomAgentConfig::new(
        "subspace",
        include_str!("../../assets/subspace.agent.md"),
    )];
    for agent in discovered.agents {
        agents.extend(convert_agent(agent, &stage, settings.enable_skills)?);
    }
    Ok(agents)
}

pub(super) fn collect_agent_files(directories: Vec<PathBuf>) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for directory in directories {
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err("Cannot read a configured Copilot agent directory.".into()),
        };
        for entry in entries {
            let entry = entry.map_err(|_| "Cannot read a configured agent file.".to_string())?;
            let name = entry.file_name();
            if !name.to_string_lossy().ends_with(".agent.md") {
                continue;
            }
            if files.len() >= MAX_AGENT_FILES {
                return Err("Configure at most 64 agent prompt files.".into());
            }
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
}

pub(super) fn stage_agent_catalog(
    settings: &config::CopilotConfig,
    workspace: &Path,
) -> Result<PathBuf, String> {
    let catalog = workspace.join(format!(
        "agent-catalog-{}-{}",
        std::process::id(),
        next_id()
    ));
    private_directory(&catalog)?;
    private_directory(&catalog.join(".github"))?;
    let stage = catalog.join(".github/agents");
    private_directory(&stage)?;
    collect_agent_files(settings.agents())?
        .into_iter()
        .try_for_each(|file| stage_agent_file(&file, &stage))?;
    Ok(catalog)
}

pub(super) fn stage_agent_file(file: &Path, stage: &Path) -> Result<(), String> {
    let content = read_agent_prompt(file)?;
    super::prompts::reject_secret_context(contains_secret(&content))?;
    let name = file
        .file_name()
        .ok_or_else(|| "Agent prompt needs a filename.".to_string())?;
    state::write_atomic(&stage.join(name), &content)
        .map_err(|_| "Cannot stage agent prompt.".into())
}

pub(super) fn read_agent_prompt(file: &Path) -> Result<String, String> {
    let metadata =
        fs::symlink_metadata(file).map_err(|_| "Cannot inspect agent prompt file.".to_string())?;
    if !metadata.is_file() || metadata.len() > PROMPT_LIMIT as u64 {
        return Err("Agent prompts must be regular files under 256 KiB.".into());
    }
    let mut content = String::new();
    fs::File::open(file)
        .and_then(|file| {
            file.take(PROMPT_LIMIT as u64 + 1)
                .read_to_string(&mut content)
        })
        .map_err(|_| "Cannot read agent prompt as UTF-8.".to_string())?;
    if content.len() > PROMPT_LIMIT {
        return Err("Agent prompt exceeds 256 KiB.".into());
    }
    Ok(content)
}
