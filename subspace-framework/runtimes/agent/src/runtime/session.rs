use super::{
    ChatId, Client, NameSetAutoRequest, NameSetRequest, Path, PathBuf, PermissionMode,
    PermissionsSetModeRequest, ResumeSessionConfig, SandboxConfig, SandboxConfigAuth,
    SandboxConfigSource, Session, SessionConfig, SessionId, SessionSetup,
    SessionUpdateOptionsParams, config, private_directory,
};
#[path = "session_access.rs"]
mod access;
pub(super) use access::{AccessError, check_access_mode, set_yolo};
/// Copilot must accept the token for the verified owner.
pub(super) fn verify_identity(
    authenticated: bool,
    reported: Option<&str>,
    verified: &str,
) -> Result<(), String> {
    if !authenticated {
        return Err(
            "Copilot did not accept the GitHub account. Check Copilot access for it.".into(),
        );
    }
    if reported.is_some_and(|login| !login.eq_ignore_ascii_case(verified)) {
        return Err("Copilot is authenticated as a different user from the chosen account. Chat is blocked; no identity fallback was attempted.".into());
    }
    Ok(())
}

pub(super) async fn open_session(
    client: &Client,
    saved: Option<SessionId>,
    setup: &SessionSetup,
    chat: ChatId,
) -> Result<Session, String> {
    if let Some(id) = saved {
        return super::canvas_negotiation::resume(client, setup, chat, id).await;
    }
    super::canvas_negotiation::create(client, setup, chat).await
}

/// The chat's current name, when it has one.
pub(super) async fn session_name(session: &Session) -> Option<String> {
    session.rpc().name().get().await.ok()?.name
}

/// The name of a saved chat that is not open. The wire call carries the
/// session id, so the session does not have to be resumed to read it.
pub(super) async fn saved_session_name(client: &Client, id: &SessionId) -> Option<String> {
    let result = client
        .call(
            "session.name.get",
            Some(serde_json::json!({ "sessionId": id })),
        )
        .await
        .ok()?;
    result["name"].as_str().map(str::to_owned)
}

/// Names a saved chat without resuming it: `session.name.set` takes the
/// session id on the wire, so no `Session` has to be opened for it.
pub(super) async fn rename_saved_session(
    client: &Client,
    id: &SessionId,
    name: &str,
) -> Result<(), String> {
    client
        .call(
            "session.name.set",
            Some(serde_json::json!({ "sessionId": id, "name": name })),
        )
        .await
        .map(|_| ())
        .map_err(|_| "Copilot did not accept this chat name.".to_string())
}

/// Sets a name Zack chose. The SDK keeps it over any later automatic one.
pub(super) async fn set_session_name(session: &Session, name: String) -> Result<(), String> {
    session
        .rpc()
        .name()
        .set(NameSetRequest { name })
        .await
        .map_err(|_| "Copilot did not accept this chat name.".to_string())
}

/// Offers a title the SDK adopts only while the chat has no name of its own.
pub(super) async fn set_automatic_name(session: &Session, summary: String) -> bool {
    session
        .rpc()
        .name()
        .set_auto(NameSetAutoRequest { summary })
        .await
        .map(|result| result.applied)
        .unwrap_or(false)
}

pub(super) fn creation_config(setup: &SessionSetup, chat: ChatId) -> SessionConfig {
    let definitions = setup.tools(chat);
    let mut tool_names: Vec<String> = definitions.iter().map(|tool| tool.name.clone()).collect();
    tool_names.push("ask_user".into());
    tool_names.extend(["list_canvas_capabilities".into(), "open_canvas".into()]);
    // The CLI's own tools: the shell, the file tools, skills, web_fetch and
    // the sub-agent tools. They run under the sandbox and the permission
    // handler, which decides each request.
    tool_names.extend(setup.builtin_tools.iter().cloned());
    let mut config = SessionConfig::default()
        .with_working_directory(setup.workspace.clone())
        .with_enable_managed_settings(true)
        .with_enable_config_discovery(false)
        .with_permission_handler(setup.decisions.clone())
        .with_user_input_handler(setup.decisions.clone())
        .with_streaming(true)
        .with_tools(definitions)
        .with_available_tools(tool_names)
        .with_custom_agents(setup.agents.clone())
        .with_custom_agents_local_only(true)
        .with_agent("subspace")
        .with_enable_file_hooks(false)
        .with_enable_host_git_operations(false)
        .with_enable_on_demand_instruction_discovery(false)
        // Instruction files load from the Copilot user location only.
        .with_skip_custom_instructions(false)
        .with_enable_skills(setup.settings.enable_skills)
        .with_plugin_directories(Vec::<PathBuf>::new())
        .with_instruction_directories(config::instruction_directories());
    config.config_directory = Some(config_directory(&setup.workspace, &setup.login));
    config.hooks = Some(false);
    config.included_builtin_skills = Some(Vec::new());
    config.skill_directories = Some(setup.settings.skills());
    config.model = setup.settings.model.clone();
    super::canvas::configure(config, chat, setup.decisions.events.clone())
}

/// Creates the account's Copilot home before the client starts, so an older
/// shared session store moves in before the CLI creates an empty one.
pub(super) fn prepare_account_directory(workspace: &Path, login: &str) -> Result<(), String> {
    private_directory(&workspace.join("runtime-config"))?;
    private_directory(&config_directory(workspace, login))?;
    adopt_shared_sessions(workspace, login)
}

/// One Copilot config directory per account, so session lists never mix
/// accounts. It is both the client's `COPILOT_HOME` and every session's
/// config directory, so the saved chat list reads the store chats write to.
pub(super) fn config_directory(workspace: &Path, login: &str) -> PathBuf {
    workspace
        .join("runtime-config")
        .join(login.to_ascii_lowercase())
}

/// Before per-account directories, every account shared one session store.
/// The first account to start keeps it, so no saved chat is lost; later
/// accounts start with their own. Does nothing once an account has a store.
pub(super) fn adopt_shared_sessions(workspace: &Path, login: &str) -> Result<(), String> {
    let shared = workspace.join("runtime-config").join("session-state");
    let account = config_directory(workspace, login).join("session-state");
    if account.exists() || !shared.is_dir() {
        return Ok(());
    }
    std::fs::rename(&shared, &account)
        .map_err(|_| "Cannot move the existing Copilot session store to this account.".into())
}

pub(super) fn resume_config(
    setup: &SessionSetup,
    chat: ChatId,
    id: SessionId,
) -> ResumeSessionConfig {
    let source = creation_config(setup, chat);
    let mut config = ResumeSessionConfig::new(id);
    config.working_directory = source.working_directory;
    config.enable_managed_settings = source.enable_managed_settings;
    config.enable_config_discovery = source.enable_config_discovery;
    config.permission_handler = source.permission_handler;
    config.user_input_handler = source.user_input_handler;
    config.streaming = source.streaming;
    config.request_canvas_renderer = source.request_canvas_renderer;
    config.canvases = source.canvases;
    config.canvas_provider = source.canvas_provider;
    config.canvas_handler = source.canvas_handler;
    config.tools = source.tools;
    config.available_tools = source.available_tools;
    config.custom_agents = source.custom_agents;
    config.custom_agents_local_only = source.custom_agents_local_only;
    config.agent = source.agent;
    config.enable_file_hooks = source.enable_file_hooks;
    config.enable_host_git_operations = source.enable_host_git_operations;
    config.enable_on_demand_instruction_discovery = source.enable_on_demand_instruction_discovery;
    config.skip_custom_instructions = source.skip_custom_instructions;
    config.enable_skills = source.enable_skills;
    config.plugin_directories = source.plugin_directories;
    config.instruction_directories = source.instruction_directories;
    config.config_directory = source.config_directory;
    config.hooks = source.hooks;
    config.included_builtin_skills = source.included_builtin_skills;
    config.skill_directories = source.skill_directories;
    // A resumed SDK session owns its model choice.
    config
}

pub(super) async fn configure_sandbox(session: &Session, enabled: bool) -> Result<(), String> {
    let mut sandbox = SandboxConfig::default();
    sandbox.enabled = enabled;
    sandbox.allow_bypass = Some(false);
    sandbox.allow_dev_tool_access = Some(false);
    sandbox.sandbox_mcp_servers = Some(true);
    sandbox.sandbox_lsp_servers = Some(true);
    // `gh` and `git` in the sandboxed shell act as the chat's account without
    // seeing its token: the child gets a fake credential and the local proxy
    // substitutes the real one only at the GitHub hosts. Ignored while the
    // sandbox is off, which is why YOLO gets no masking.
    sandbox.auth = Some(SandboxConfigAuth {
        gh: Some(true),
        git: Some(true),
    });
    let result = session
        .rpc()
        .options()
        .update(SessionUpdateOptionsParams {
            sandbox_config: Some(sandbox),
            sandbox_config_source: Some(SandboxConfigSource::SessionFlag),
            ..Default::default()
        })
        .await
        .map_err(|_| {
            "The SDK could not apply the requested sandbox policy. Chat is blocked.".to_string()
        })?;
    if !result.success {
        return Err("Copilot rejected the sandbox change. Chat is blocked.".into());
    }
    Ok(())
}

pub(super) async fn verify_local_session(
    session: &Session,
    workspace: &Path,
) -> Result<(), String> {
    let snapshot = session
        .rpc()
        .metadata()
        .snapshot()
        .await
        .map_err(|_| "Cannot verify the local SDK session. Chat is blocked.".to_string())?;
    if snapshot.already_in_use || snapshot.is_remote {
        return Err("The saved SDK chat is remote or already used by another process. Close that connection before starting Subspace chat.".into());
    }
    let actual = Path::new(&snapshot.working_directory)
        .canonicalize()
        .map_err(|_| "Cannot verify the SDK workspace.".to_string())?;
    let expected = workspace
        .canonicalize()
        .map_err(|_| "Cannot verify the private workspace.".to_string())?;
    if actual != expected {
        return Err(
            "The SDK session does not use Subspace's private workspace. Chat is blocked.".into(),
        );
    }
    Ok(())
}

pub(super) async fn ensure_managed_policy(session: &Session) -> Result<(), String> {
    access::check_managed_policy(session)
        .await
        .map_err(|error| error.message)
}

pub(super) async fn reset_permissions(session: &Session) -> Result<(), String> {
    let result = session
        .rpc()
        .permissions()
        .set_mode(PermissionsSetModeRequest {
            mode: PermissionMode::Manual,
            source: None,
            assisted_approval_model: None,
        })
        .await
        .map_err(|_| "Cannot reset Copilot permissions. Chat is blocked.".to_string())?;
    if !result.success || result.mode != PermissionMode::Manual {
        return Err("Copilot did not confirm manual permissions. Chat is blocked.".into());
    }
    Ok(())
}

pub(super) async fn current_yolo(session: &Session) -> Result<bool, String> {
    let mode = session
        .rpc()
        .permissions()
        .get_mode()
        .await
        .map_err(|_| "Copilot access mode cannot be verified. Chat is blocked.".to_string())?;
    Ok(mode.mode == PermissionMode::AllowAll)
}

#[cfg(test)]
mod tests {
    use super::{adopt_shared_sessions, config_directory, creation_config, resume_config};
    use crate::runtime::decisions::Decisions;
    use crate::runtime::setup::SessionSetup;
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use subspace_agent_tools::{Access, AgentTool, Catalog, Invocation, ToolSpec};

    fn board_tool(name: &'static str) -> Arc<dyn AgentTool> {
        Arc::new(Named(ToolSpec {
            name,
            title: name,
            description: name,
            access: Access::Read,
            arguments: subspace_agent_tools::Arguments::Single { fields: Vec::new() },
            approval: subspace_agent_tools::ApprovalPolicy::None,
        }))
    }

    struct Named(ToolSpec);

    #[async_trait::async_trait]
    impl AgentTool for Named {
        fn spec(&self) -> &ToolSpec {
            &self.0
        }
        async fn call(&self, _invocation: Invocation) -> Result<serde_json::Value, String> {
            Ok(serde_json::json!({}))
        }
    }

    fn setup(builtin_tools: Vec<String>) -> SessionSetup {
        SessionSetup {
            settings: crate::runtime::config::CopilotConfig::default(),
            workspace: PathBuf::from("/private/workspace"),
            login: "work_corp".into(),
            decisions: Arc::new(Decisions {
                events: Arc::new(crate::runtime::EventQueue::default()),
                pending: Mutex::new(HashMap::new()),
                chats: Mutex::new(HashMap::new()),
                workspace: PathBuf::from("/private/workspace"),
            }),
            agents: Vec::new(),
            builtin_tools,
            tools: Catalog::new(vec![board_tool("ado_read"), board_tool("ado_write")]).unwrap(),
            runtime_version: None,
            activity: Default::default(),
        }
    }

    #[test]
    fn a_session_offers_the_board_tools_ask_user_and_the_cli_tools() {
        let builtin: Vec<String> = crate::runtime::builtins::FALLBACK_BUILTIN_TOOLS
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        let config = creation_config(&setup(builtin.clone()), 1);
        let available = config.available_tools.unwrap();
        for name in ["ado_read", "ado_write", "ask_user"] {
            assert!(available.iter().any(|tool| tool == name), "{name}");
        }
        let registered: Vec<&str> = config
            .tools
            .iter()
            .flatten()
            .map(|tool| tool.name.as_str())
            .collect();
        assert_eq!(registered, ["ado_read", "ado_write"]);
        for name in [
            "bash",
            "view",
            "glob",
            "grep",
            "create",
            "edit",
            "web_fetch",
            "task",
        ] {
            assert!(available.iter().any(|tool| tool == name), "{name}");
        }
        assert_eq!(available.len(), builtin.len() + 5);
        // The custom agent keeps every offered tool: it declares no list.
        assert!(config.custom_agents.unwrap_or_default().is_empty());
    }

    #[test]
    fn a_session_without_discovered_cli_tools_still_has_the_board_tools() {
        let config = creation_config(&setup(Vec::new()), 1);
        let available = config.available_tools.unwrap();
        assert_eq!(
            available,
            [
                "ado_read",
                "ado_write",
                "ask_user",
                "list_canvas_capabilities",
                "open_canvas"
            ]
        );
    }

    #[test]
    fn new_and_resumed_sessions_keep_custom_agents_local() {
        let setup = setup(Vec::new());
        let created = creation_config(&setup, 1);
        let resumed = resume_config(&setup, 1, "session-1".into());
        assert_eq!(created.custom_agents_local_only, Some(true));
        assert_eq!(resumed.custom_agents_local_only, Some(true));
        assert_eq!(created.enable_config_discovery, Some(false));
        assert_eq!(resumed.enable_config_discovery, Some(false));
        assert_eq!(created.agent.as_deref(), Some("subspace"));
        assert_eq!(resumed.agent.as_deref(), Some("subspace"));
        assert_eq!(created.request_canvas_renderer, Some(true));
        assert_eq!(resumed.request_canvas_renderer, Some(true));
        assert_eq!(created.canvas_provider, resumed.canvas_provider);
        let declarations = created.canvases.unwrap();
        assert_eq!(declarations.len(), 3);
        assert!(
            declarations
                .iter()
                .all(|canvas| canvas.actions.as_ref().is_some_and(Vec::is_empty))
        );
        assert!(resumed.canvas_handler.is_some());
    }

    #[test]
    fn the_existing_session_store_moves_once_and_only_to_the_first_account() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        let shared = workspace.join("runtime-config/session-state");
        fs::create_dir_all(shared.join("chat-1")).unwrap();
        fs::create_dir_all(config_directory(workspace, "first")).unwrap();
        adopt_shared_sessions(workspace, "first").unwrap();
        assert!(
            config_directory(workspace, "first")
                .join("session-state/chat-1")
                .is_dir()
        );
        assert!(!shared.exists());
        fs::create_dir_all(config_directory(workspace, "second")).unwrap();
        adopt_shared_sessions(workspace, "second").unwrap();
        assert!(
            !config_directory(workspace, "second")
                .join("session-state")
                .exists()
        );
        assert!(adopt_shared_sessions(workspace, "first").is_ok());
    }

    #[test]
    fn each_account_gets_its_own_copilot_config_directory() {
        let workspace = Path::new("/private/workspace");
        assert_eq!(
            config_directory(workspace, "Work_Corp"),
            PathBuf::from("/private/workspace/runtime-config/work_corp")
        );
        assert_ne!(
            config_directory(workspace, "other"),
            config_directory(workspace, "work_corp")
        );
    }
}
