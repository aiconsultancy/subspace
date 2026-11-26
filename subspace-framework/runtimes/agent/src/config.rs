//! Private Copilot settings. These never enter UI preferences or transcripts.
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use github_copilot_sdk::types::McpServerConfig;
use serde::Deserialize;
use subspace_core::Secret;

const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
#[cfg(unix)]
const PRIVATE_ACCESS_MASK: u32 = 0o077;
const MAX_SKILL_DIRECTORIES: usize = 32;
const MAX_MCP_SERVERS: usize = 16;
const MAX_AGENT_DIRECTORIES: usize = 16;

#[derive(Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CopilotConfig {
    pub token: Option<Secret>,
    /// Required for ambient authentication, independently of the dashboard account.
    pub login: Option<String>,
    pub model: Option<String>,
    pub agent_directories: Option<Vec<PathBuf>>,
    #[serde(default = "enabled")]
    pub enable_skills: bool,
    pub skill_directories: Vec<PathBuf>,
    pub enable_mcp: bool,
    pub mcp_servers: BTreeMap<String, McpServerConfig>,
}

fn enabled() -> bool {
    true
}

impl Default for CopilotConfig {
    fn default() -> Self {
        CopilotConfig {
            token: None,
            login: None,
            model: None,
            agent_directories: None,
            enable_skills: true,
            skill_directories: Vec::new(),
            enable_mcp: false,
            mcp_servers: BTreeMap::new(),
        }
    }
}

pub(super) fn path() -> Result<PathBuf, String> {
    subspace_core::config::path()
        .map(|path| path.with_file_name("copilot.json"))
        .ok_or_else(|| "Cannot find the app's private config directory.".into())
}

impl CopilotConfig {
    pub fn load() -> Result<Self, String> {
        CopilotConfig::load_from(&path()?)
    }

    fn load_from(path: &Path) -> Result<Self, String> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(_) => return Err("Cannot read private copilot.json settings.".into()),
        };
        if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
            return Err("copilot.json must be a regular file smaller than 1 MiB.".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & PRIVATE_ACCESS_MASK != 0 {
                return Err("copilot.json must be private. Set its permissions to 600.".into());
            }
        }
        let mut text = String::new();
        let file = fs::File::open(path)
            .map_err(|_| "Cannot read private copilot.json settings.".to_string())?;
        let _opened = file
            .metadata()
            .map_err(|_| "Cannot inspect private copilot.json settings.".to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            if metadata.dev() != _opened.dev()
                || metadata.ino() != _opened.ino()
                || _opened.permissions().mode() & PRIVATE_ACCESS_MASK != 0
            {
                return Err(
                    "copilot.json changed or is not private. Check its permissions and try again."
                        .into(),
                );
            }
        }
        file.take(MAX_CONFIG_BYTES + 1)
            .read_to_string(&mut text)
            .map_err(|_| "Cannot read private copilot.json settings.".to_string())?;
        if text.len() as u64 > MAX_CONFIG_BYTES {
            return Err("copilot.json exceeds 1 MiB.".into());
        }
        let config: Self = serde_json::from_str(&text).map_err(|error| {
            format!(
                "Invalid copilot.json at line {}, column {}. Check field names and types.",
                error.line(),
                error.column()
            )
        })?;
        #[cfg(not(unix))]
        if config.token.is_some() {
            return Err("PAT file access controls are not verified on this platform yet. Use a verified ambient login.".into());
        }
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), String> {
        if let Some(token) = &self.token {
            validate_token(token.expose())?;
        }
        if let Some(login) = &self.login
            && (login.is_empty()
                || !login
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'))
        {
            return Err("copilot.json login must be a GitHub user name.".into());
        }
        if self.skill_directories.len() > MAX_SKILL_DIRECTORIES
            || self.mcp_servers.len() > MAX_MCP_SERVERS
        {
            return Err("Configure at most 32 skill directories and 16 MCP servers.".into());
        }
        if self
            .agent_directories
            .as_ref()
            .is_some_and(|dirs| dirs.len() > MAX_AGENT_DIRECTORIES)
        {
            return Err("Configure at most 16 agent directories.".into());
        }
        Ok(())
    }

    pub fn agents(&self) -> Vec<PathBuf> {
        self.agent_directories
            .clone()
            .unwrap_or_else(|| {
                dirs::home_dir().map_or_else(Vec::new, |home| {
                    vec![home.join(".config/subspace/copilot/agents")]
                })
            })
            .into_iter()
            .map(expand_home)
            .collect()
    }

    /// Skills from the Copilot and agent user locations plus configured
    /// directories. Empty while skills are disabled.
    pub fn skills(&self) -> Vec<PathBuf> {
        if !self.enable_skills {
            return Vec::new();
        }
        let mut directories = default_skill_directories();
        for configured in &self.skill_directories {
            let directory = expand_home(configured.clone());
            if !directories.contains(&directory) {
                directories.push(directory);
            }
        }
        directories
    }
}

fn default_skill_directories() -> Vec<PathBuf> {
    dirs::home_dir().map_or_else(Vec::new, |home| {
        vec![home.join(".copilot/skills"), home.join(".agents/skills")]
    })
}

/// The Copilot user location. Repository instruction discovery stays off.
pub(super) fn instruction_directories() -> Vec<PathBuf> {
    dirs::home_dir().map_or_else(Vec::new, |home| vec![home.join(".copilot")])
}

pub(super) fn expand_home(path: PathBuf) -> PathBuf {
    match path.strip_prefix("~") {
        Ok(suffix) => dirs::home_dir().map_or(path.clone(), |home| home.join(suffix)),
        Err(_) => path,
    }
}

pub(super) fn validate_token(token: &str) -> Result<(), String> {
    if token.starts_with("github_pat_")
        && token.len() > "github_pat_".len()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        Ok(())
    } else {
        Err("Copilot requires a nonempty fine-grained PAT, not a classic PAT. Stored OAuth login is also supported.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::{CopilotConfig, expand_home, validate_token};
    use std::path::PathBuf;

    #[test]
    fn default_agents_only_use_the_subspace_directory() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(
            CopilotConfig::default().agents(),
            vec![home.join(".config/subspace/copilot/agents")]
        );
    }

    #[test]
    fn agent_directories_use_explicit_values_and_expand_home() {
        let config: CopilotConfig =
            serde_json::from_str(r#"{"agentDirectories":["~/agents","/explicit/agents"]}"#)
                .unwrap();
        assert_eq!(
            config.agents(),
            vec![
                expand_home(PathBuf::from("~/agents")),
                PathBuf::from("/explicit/agents")
            ]
        );
        assert_eq!(
            expand_home(PathBuf::from("/explicit/skills")),
            PathBuf::from("/explicit/skills")
        );
        assert_eq!(
            expand_home(PathBuf::from("~someone/skills")),
            PathBuf::from("~someone/skills")
        );
    }
    use std::fs;

    #[test]
    fn skills_are_on_by_default_but_mcp_still_needs_opt_in() {
        let config: CopilotConfig =
            serde_json::from_str(r#"{"skillDirectories":["/skills"]}"#).unwrap();
        assert!(config.enable_skills);
        assert!(config.skills().contains(&PathBuf::from("/skills")));
        assert!(!config.enable_mcp);
        assert!(config.token.is_none());
        assert!(config.validate().is_ok());
        let off: CopilotConfig = serde_json::from_str(r#"{"enableSkills":false}"#).unwrap();
        assert!(off.skills().is_empty());
    }

    #[test]
    fn enterprise_managed_work_login_is_supported() {
        let config: CopilotConfig =
            serde_json::from_str(r#"{"login":"work-user_company"}"#).unwrap();
        assert!(config.validate().is_ok());
        let invalid: CopilotConfig =
            serde_json::from_str(r#"{"login":"work-user/company"}"#).unwrap();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn rejects_classic_and_malformed_pat_without_disclosing_it() {
        assert!(validate_token("github_pat_example123").is_ok());
        for token in ["ghp_do_not_disclose", "github_pat_", "github_pat_a\n"] {
            let error = validate_token(token).unwrap_err();
            assert!(!error.contains(token));
        }
    }

    #[test]
    fn config_errors_do_not_echo_secret_values() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("copilot.json");
        fs::write(&file, r#"{"token": "ghp_do_not_disclose", "login": 3}"#).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let error = CopilotConfig::load_from(&file).err().unwrap();
        assert!(!error.contains("do_not_disclose"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_shared_config_even_without_a_token() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("copilot.json");
        fs::write(&file, "{}").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(CopilotConfig::load_from(&file).is_err());
    }
}
