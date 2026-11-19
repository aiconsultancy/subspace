//! Which program each ACP agent preset runs, and its process command.
use super::{BTreeMap, Path, PathBuf, ProcessCommand, Stdio};
use subspace_core::config::{AcpAgent, AcpConfig};

/// The program and arguments that start one agent's ACP server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Launch {
    pub(super) executable: PathBuf,
    pub(super) args: Vec<String>,
    pub(super) env: BTreeMap<String, String>,
}

type Candidate = (&'static str, &'static [&'static str]);

/// A preset's programs, tried in order, and how to install one.
struct Preset {
    candidates: &'static [Candidate],
    install: &'static str,
}

/// An installed adapter comes first, then `npx`, which fetches the published
/// adapter on first use.
fn preset(agent: AcpAgent) -> Preset {
    let (candidates, install): (&'static [Candidate], &'static str) = match agent {
        AcpAgent::OpenCode => (
            &[("opencode", &["acp"])],
            "Install OpenCode so `opencode` is on your PATH",
        ),
        AcpAgent::Claude => (
            &[
                ("claude-agent-acp", &[]),
                ("npx", &["-y", "@agentclientprotocol/claude-agent-acp"]),
            ],
            "Install Node.js for `npx`, or run `npm install -g @agentclientprotocol/claude-agent-acp`",
        ),
        AcpAgent::Codex => (
            &[
                ("codex-acp", &[]),
                ("npx", &["-y", "@agentclientprotocol/codex-acp"]),
            ],
            "Install Node.js for `npx`, or run `npm install -g @agentclientprotocol/codex-acp`",
        ),
        AcpAgent::Gemini => (
            &[("gemini", &["--acp"])],
            "Install Gemini CLI with `npm install -g @google/gemini-cli`",
        ),
        AcpAgent::Custom => (&[], "Set chat.acp.command to the agent's executable"),
    };
    Preset {
        candidates,
        install,
    }
}

/// Resolves the configured agent through the shared tool lookup. A
/// `command` replaces the preset's program, and `args` its arguments.
pub(super) fn resolve(
    config: &AcpConfig,
    find: impl Fn(&str) -> Option<PathBuf>,
) -> Result<Launch, String> {
    let preset = preset(config.agent);
    let found = match config.command.as_deref() {
        Some(command) => find(command).map(|path| (path, preset_args(preset.candidates.first()))),
        None => preset
            .candidates
            .iter()
            .find_map(|(name, args)| find(name).map(|path| (path, to_strings(args)))),
    };
    let Some((executable, args)) = found else {
        return Err(format!(
            "{} was not found. {}, or set chat.acp.command in the config file, then restart Subspace.",
            config.agent.label(),
            preset.install
        ));
    };
    Ok(Launch {
        executable,
        args: config.args.clone().unwrap_or(args),
        env: config.env.clone(),
    })
}

fn preset_args(candidate: Option<&Candidate>) -> Vec<String> {
    candidate
        .map(|(_, args)| to_strings(args))
        .unwrap_or_default()
}

fn to_strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_owned()).collect()
}

pub(super) fn find(name: &str) -> Option<PathBuf> {
    subspace_core::path::find_tool(name).ok()
}

pub(super) fn command(launch: &Launch, cwd: &Path) -> ProcessCommand {
    let mut command = crate::runtime::child::agent_process(&launch.executable);
    command
        .args(&launch.args)
        .envs(&launch.env)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}
