//! Replaying a Copilot cloud agent session. The SDK connects to the task id,
//! which gives a transient local session holding the last remote session's
//! events. Nothing is sent to it and nothing is saved: the connection changes
//! no server state and the replay is read-only.

use github_copilot_sdk::rpc::{ConnectRemoteSessionParams, ConnectedRemoteSessionMetadata};
use subspace_client_github::copilot_agents::validate_task_id;

use super::{ChatCommand, Client, ResumeSessionConfig, Session, SessionId};

/// What a failed connect says. It names the two places the session can still
/// be read, because Subspace cannot replay it.
pub(super) const CONNECT_FAILED: &str = concat!(
    "Cannot replay this cloud agent session. ",
    "Open the task's session page on GitHub, or read its log in the Copilot Agents dashboard."
);

/// The title a replay carries when the session has no name of its own.
const UNNAMED_REPLAY: &str = "Cloud agent session";

/// The task id, checked before any request. Task ids are opaque, so the
/// GitHub client's own charset and length check is the whole rule.
pub(super) fn checked_task_id(task_id: &str) -> Result<String, String> {
    match validate_task_id(task_id) {
        Ok(()) => Ok(task_id.to_owned()),
        Err(_) => Err("That is not a cloud agent task id.".to_string()),
    }
}

/// The resume configuration of a replay: no workspace, no tools, no
/// permission handler and no agent, because nothing in this session runs.
/// `available_tools` has to be present and empty under `ClientMode::Empty`.
pub(super) fn replay_config(session: SessionId) -> ResumeSessionConfig {
    let mut config = ResumeSessionConfig::new(session);
    config.available_tools = Some(Vec::new());
    config.streaming = Some(true);
    config
}

/// The tab's title: the session's own name, else the repository it ran in.
pub(super) fn replay_title(metadata: &ConnectedRemoteSessionMetadata) -> String {
    if let Some(name) = metadata
        .name
        .as_ref()
        .filter(|name| !name.trim().is_empty())
    {
        return name.trim().to_owned();
    }
    let repository = &metadata.repository;
    if repository.owner.is_empty() || repository.name.is_empty() {
        return UNNAMED_REPLAY.to_owned();
    }
    format!("{}/{}", repository.owner, repository.name)
}

/// A replay rejects everything but closing its tab: the connected session
/// reports `remoteSteerable: false`, so steering stays on the task's page.
pub(super) fn reject_read_only(command: &ChatCommand, read_only: bool) -> Result<(), String> {
    if !read_only || matches!(command, ChatCommand::Close) {
        return Ok(());
    }
    Err(
        "This tab replays a finished cloud agent session and is read-only. Steer the task on its session page on GitHub."
            .into(),
    )
}

/// Whether a chat is one of the tabs a restart reopens. A replay never is:
/// its session is transient and nothing in it was sent.
pub(super) fn saved_as_tab(persisted: bool, read_only: bool) -> bool {
    persisted && !read_only
}

/// Connects to the cloud task and resumes what it reports, giving the
/// replay session and the title its metadata carries.
pub(super) async fn connect_replay(
    client: &Client,
    task_id: &str,
) -> Result<(Session, String), String> {
    let session_id = checked_task_id(task_id)?;
    let connected = client
        .rpc()
        .sessions()
        .connect(ConnectRemoteSessionParams {
            session_id: session_id.into(),
        })
        .await
        .map_err(|_| CONNECT_FAILED.to_string())?;
    let title = replay_title(&connected.metadata);
    let session = client
        .resume_session(replay_config(connected.session_id))
        .await
        .map_err(|_| CONNECT_FAILED.to_string())?;
    Ok((session, title))
}

#[cfg(test)]
mod tests {
    use super::{checked_task_id, reject_read_only, replay_config, replay_title, saved_as_tab};
    use crate::ChatCommand;
    use github_copilot_sdk::rpc::{
        ConnectedRemoteSessionMetadata, ConnectedRemoteSessionMetadataRepository,
    };

    fn metadata(name: Option<&str>, owner: &str, repo: &str) -> ConnectedRemoteSessionMetadata {
        ConnectedRemoteSessionMetadata {
            name: name.map(str::to_owned),
            repository: ConnectedRemoteSessionMetadataRepository {
                branch: "main".into(),
                name: repo.into(),
                owner: owner.into(),
            },
            ..Default::default()
        }
    }

    #[test]
    fn only_an_opaque_task_id_reaches_the_sdk() {
        assert_eq!(
            checked_task_id("3f0a-91b2").as_deref(),
            Ok("3f0a-91b2"),
            "the id GitHub reports is passed through"
        );
        assert!(checked_task_id("").is_err());
        assert!(checked_task_id("../../etc/passwd").is_err());
        assert!(checked_task_id("one two").is_err());
        assert!(checked_task_id(&"a".repeat(101)).is_err());
    }

    #[test]
    fn a_replay_resumes_with_no_tools_and_no_workspace() {
        let config = replay_config("transient-1".into());
        assert_eq!(
            config.available_tools,
            Some(Vec::new()),
            "the empty list is required, not absent"
        );
        assert_eq!(config.streaming, Some(true));
        assert!(config.permission_handler.is_none());
        assert!(config.working_directory.is_none());
        assert!(config.agent.is_none());
    }

    #[test]
    fn the_tab_takes_the_sessions_name_and_otherwise_its_repository() {
        assert_eq!(
            replay_title(&metadata(Some(" Fix the flaky test "), "shell", "subspace")),
            "Fix the flaky test"
        );
        assert_eq!(
            replay_title(&metadata(None, "shell", "subspace")),
            "shell/subspace"
        );
        assert_eq!(
            replay_title(&metadata(Some("  "), "", "")),
            "Cloud agent session"
        );
    }

    #[test]
    fn a_replay_refuses_everything_but_closing_its_tab() {
        let send = ChatCommand::Send {
            text: "try again".into(),
            context: Vec::new(),
            images: Vec::new(),
        };
        let refused = reject_read_only(&send, true).unwrap_err();
        assert!(refused.contains("read-only"));
        assert!(refused.contains("session page"));
        assert!(reject_read_only(&ChatCommand::Rename("other".into()), true).is_err());
        assert!(reject_read_only(&ChatCommand::SetYolo(true), true).is_err());
        assert!(reject_read_only(&ChatCommand::Close, true).is_ok());
        assert!(reject_read_only(&send, false).is_ok(), "an ordinary chat");
    }

    #[test]
    fn a_replay_is_never_one_of_the_saved_tabs() {
        assert!(
            !saved_as_tab(true, true),
            "a replay stays out of copilot-tabs, however it looks"
        );
        assert!(!saved_as_tab(false, true));
        assert!(saved_as_tab(true, false), "a chat with a sent message");
        assert!(!saved_as_tab(false, false), "a chat that sent nothing");
    }
}
