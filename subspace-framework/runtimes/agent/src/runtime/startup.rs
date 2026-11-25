#[path = "startup/identity.rs"]
mod identity;

pub(super) use identity::{Identity, resolve_identity};

use super::export::open_session_events;
use super::{
    ChatCommand, ChatEvent, ChatId, Client, ClientMode, ClientOptions, Event, EventQueue,
    MAX_CHATS, Path, Secret, Session, Transport, config, configure_sandbox, ensure_managed_policy,
    history_entries, reset_permissions, session::config_directory, verify_identity,
    verify_local_session,
};
use github_copilot_sdk::types::DeliveryMode;
pub(super) fn validate_startup(
    settings: &config::CopilotConfig,
    override_runtime: bool,
) -> Result<(), String> {
    if override_runtime {
        return Err(
            "Unset COPILOT_CLI_PATH: Subspace requires its bundled Copilot 1.0.105 runtime.".into(),
        );
    }
    if settings.enable_mcp && !settings.mcp_servers.is_empty() {
        return Err("Explicit MCP startup is blocked: SDK 1.0.16 cannot establish high-level pre-create confinement. Disable enableMcp until this SDK gap is resolved.".into());
    }
    Ok(())
}

/// Whether another chat may be opened. The panel refuses first, so this is
/// the runtime's own limit rather than the message Zack normally sees.
pub(super) fn room_for_a_tab(open: usize) -> Result<(), String> {
    if open >= MAX_CHATS {
        return Err(format!(
            "At most {MAX_CHATS} chats are open at once. Close a tab first."
        ));
    }
    Ok(())
}

/// A message steers the running turn, so it is the one command a busy chat
/// still accepts. The chat holds a setting until the turn ends (see
/// `chat_held.rs`); anything else, including a skill `/command`, is refused.
pub(super) fn validate_command(
    command: &ChatCommand,
    busy: bool,
    verified: bool,
) -> Result<(), String> {
    let allowed_while_busy = matches!(
        command,
        ChatCommand::Abort | ChatCommand::Close | ChatCommand::Send { .. }
    );
    if busy && !allowed_while_busy {
        return Err("Stop or finish the current reply before running this command.".into());
    }
    if !verified
        && matches!(
            command,
            ChatCommand::Send { .. } | ChatCommand::RunCommand { .. }
        )
    {
        return Err("Copilot policy is unverified. Select an access mode or start a new chat before sending.".into());
    }
    Ok(())
}

/// How the next message reaches Copilot. A message typed while a turn runs
/// is injected into that turn (steering); an idle chat keeps the default
/// queue, so the message starts its own turn.
pub(super) fn delivery_mode(busy: bool) -> DeliveryMode {
    if busy {
        DeliveryMode::Immediate
    } else {
        DeliveryMode::Enqueue
    }
}

pub(super) fn session_file_name(login: &str) -> String {
    format!("copilot-session-{}.json", login.to_ascii_lowercase())
}

pub(super) fn client_options(token: &Secret, workspace: &Path, login: &str) -> ClientOptions {
    let mut options = ClientOptions::default()
        .with_transport(Transport::Stdio)
        .with_mode(ClientMode::Empty)
        // Copilot keeps its state here, never in ~/.copilot.
        .with_base_directory(config_directory(workspace, login))
        .with_github_token(token.expose())
        .with_use_logged_in_user(false);
    options.working_directory = workspace.to_path_buf();
    options
}

pub(super) async fn start_client(identity: &Identity, workspace: &Path) -> Result<Client, String> {
    let client = Client::start(client_options(&identity.token, workspace, &identity.login))
        .await
        .map_err(|_| {
            "Cannot start bundled Copilot. Check runtime installation and authentication."
                .to_string()
        })?;
    let auth = client
        .get_auth_status()
        .await
        .map_err(|_| "Cannot verify the Copilot identity.".to_string())?;
    verify_identity(
        auth.is_authenticated,
        auth.login.as_deref(),
        &identity.login,
    )?;
    Ok(client)
}

pub(super) async fn sandbox_preflight(client: &Client) -> Result<(), String> {
    let support = client
        .rpc()
        .sandbox()
        .get_host_support()
        .await
        .map_err(|_| "Copilot sandbox preflight is unavailable. Chat is blocked.".to_string())?;
    if !support.supported {
        return Err(format!(
            "Copilot sandbox is unavailable. Chat is blocked. {}",
            support.reason.unwrap_or_default()
        ));
    }
    Ok(())
}

pub(super) async fn prepare_local_session(
    session: &Session,
    workspace: &Path,
) -> Result<(), String> {
    verify_local_session(session, workspace).await?;
    configure_sandbox(session, true).await?;
    ensure_managed_policy(session).await?;
    reset_permissions(session).await
}

pub(super) async fn publish_history(
    session: &Session,
    chat: ChatId,
    events: &EventQueue,
) -> Result<Vec<crate::TranscriptEntry>, String> {
    // The journal, not `session.getMessages`: the messages view carries no
    // tool calls, so a replay built from it loses them.
    let history = open_session_events(session).await?;
    let entries = history_entries(&history);
    events
        .push(Event::of(chat, ChatEvent::History(entries.clone())))
        .await;
    super::canvas::remember(&history, chat, events).await;
    Ok(entries)
}

pub(super) async fn publish_ready(session: &Session, chat: ChatId, events: &EventQueue) {
    events.push(Event::of(chat, ChatEvent::Mode(false))).await;
    events
        .push(Event::of(
            chat,
            ChatEvent::Ready {
                session_id: session.id().to_string(),
            },
        ))
        .await;
}

#[cfg(test)]
mod tests {
    use super::{
        client_options, delivery_mode, room_for_a_tab, session_file_name, validate_command,
        validate_startup,
    };

    #[test]
    fn the_runtime_refuses_a_ninth_chat_whichever_kind_it_is() {
        assert!(room_for_a_tab(0).is_ok());
        assert!(room_for_a_tab(crate::MAX_CHATS - 1).is_ok());
        assert!(
            room_for_a_tab(crate::MAX_CHATS)
                .unwrap_err()
                .contains("Close a tab")
        );
    }

    #[test]
    fn each_account_keeps_its_own_conversation() {
        assert_eq!(
            session_file_name("Work_corp"),
            "copilot-session-work_corp.json"
        );
    }
    use crate::ChatCommand;
    use crate::runtime::config::CopilotConfig;
    use github_copilot_sdk::types::DeliveryMode;
    use std::path::Path;
    use subspace_core::Secret;

    #[test]
    fn startup_blocks_executable_integrations_and_runtime_overrides_but_allows_skills() {
        let settings: CopilotConfig = serde_json::from_str(r#"{"login":"work-user"}"#).unwrap();
        assert!(settings.enable_skills);
        assert!(validate_startup(&settings, false).is_ok());
        assert!(validate_startup(&settings, true).is_err());
        let mcp: CopilotConfig = serde_json::from_str(r#"{"login":"work-user","enableMcp":true,"mcpServers":{"executable":{"type":"local","command":"must-not-run","tools":["*"]}}}"#).unwrap();
        assert!(
            validate_startup(&mcp, false)
                .unwrap_err()
                .contains("MCP startup")
        );
    }

    #[test]
    fn command_policy_blocks_turns_but_keeps_stopping_and_closing_available() {
        assert!(
            validate_command(
                &ChatCommand::Send {
                    text: "hello".into(),
                    context: Vec::new(),
                    images: Vec::new()
                },
                false,
                false
            )
            .is_err()
        );
        assert!(validate_command(&ChatCommand::SetYolo(true), true, true).is_err());
        assert!(validate_command(&ChatCommand::Abort, true, false).is_ok());
        assert!(validate_command(&ChatCommand::Close, true, false).is_ok());
        assert!(
            validate_command(
                &ChatCommand::RunCommand {
                    name: "skill".into(),
                    args: String::new()
                },
                true,
                true
            )
            .is_err(),
            "a skill command still waits for the turn to end"
        );
        assert!(
            validate_command(
                &ChatCommand::Send {
                    text: "look at the other file instead".into(),
                    context: Vec::new(),
                    images: Vec::new()
                },
                true,
                true
            )
            .is_ok(),
            "a message steers the running turn"
        );
    }

    #[test]
    fn a_message_typed_during_a_turn_is_delivered_immediately() {
        assert_eq!(delivery_mode(true), DeliveryMode::Immediate);
        assert_eq!(
            delivery_mode(false),
            DeliveryMode::Enqueue,
            "an idle chat keeps the default delivery"
        );
    }

    #[test]
    fn explicit_token_disables_ambient_identity_without_starting_client() {
        let options = client_options(
            &Secret::new("github_pat_test"),
            Path::new("/private/workspace"),
            "Work_corp",
        );
        assert_eq!(options.use_logged_in_user, Some(false));
        assert_eq!(
            options.base_directory.as_deref(),
            Some(Path::new("/private/workspace/runtime-config/work_corp")),
            "the client lists the sessions its chats save"
        );
        assert_eq!(options.working_directory, Path::new("/private/workspace"));
    }
}
