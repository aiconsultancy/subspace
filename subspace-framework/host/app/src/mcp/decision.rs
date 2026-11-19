//! The pure reconcile decision for the app server (REQ-10 to REQ-14).

use std::collections::BTreeMap;
use std::net::SocketAddr;

use subspace_core::mcp_token::TokenStatus;

/// What the MCP page reports about the app server (REQ-03).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ServerState {
    Off,
    Listening(SocketAddr),
    Error(String),
}

/// Where the app server is. A start runs on the worker until its reply, so it
/// is a state of its own: a disable must stop it, and only its own reply may
/// settle it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Lifecycle {
    /// No start waits for a reply.
    Settled(ServerState),
    /// The start with `generation` waits for its reply. The page still shows
    /// `shown`, the state before the start.
    Starting { generation: u64, shown: ServerState },
}

impl Lifecycle {
    /// What the MCP page reports.
    pub(super) fn shown(&self) -> &ServerState {
        match self {
            Self::Settled(state) | Self::Starting { shown: state, .. } => state,
        }
    }
}

/// What a listener depends on. A change needs a new listener (REQ-12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Listener {
    pub(super) address: SocketAddr,
    pub(super) require_token: bool,
    /// Counts token files created in this session, so a new token restarts.
    pub(super) token_revision: u64,
}

/// The saved policy that the app server should follow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Target {
    pub(super) enabled: bool,
    pub(super) listener: Listener,
    pub(super) groups: BTreeMap<String, bool>,
    /// The ids of the active boards whose tools the catalog serves.
    pub(super) boards: Vec<&'static str>,
    /// The favourite repositories that scope the board tools.
    pub(super) repos: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Decision {
    Noop,
    /// Serve the new groups or boards on the open listener (REQ-13).
    SwapCatalog,
    Start,
    /// Stop the listener, then start one with the new values (REQ-12).
    Restart,
    Stop,
    /// A required token file is missing or not private (REQ-09a).
    Refuse(TokenStatus),
}

/// `applied` is the target of the last start or catalog swap. A disable stops
/// a pending start; the worker runs commands in order, so the stop closes the
/// listener after the start binds. Otherwise a pending start decides as the
/// state the page shows.
pub(super) fn decide(
    lifecycle: &Lifecycle,
    applied: Option<&Target>,
    desired: &Target,
    token: TokenStatus,
) -> Decision {
    if !desired.enabled {
        return if *lifecycle == Lifecycle::Settled(ServerState::Off) {
            Decision::Noop
        } else {
            Decision::Stop
        };
    }
    if desired.listener.require_token && token != TokenStatus::Private {
        return Decision::Refuse(token);
    }
    match (lifecycle.shown(), applied) {
        (ServerState::Listening(_), Some(applied)) if applied.listener == desired.listener => {
            if applied.groups == desired.groups
                && applied.boards == desired.boards
                && applied.repos == desired.repos
            {
                Decision::Noop
            } else {
                Decision::SwapCatalog
            }
        }
        (ServerState::Listening(_), _) => Decision::Restart,
        // Off starts; Error retries (REQ-14).
        _ => Decision::Start,
    }
}

/// The page's reason for a refused token. Never contains the token value.
pub(super) fn token_reason(status: TokenStatus) -> &'static str {
    match status {
        TokenStatus::Missing => "the token file is missing. Create a token on the MCP page.",
        TokenStatus::NotPrivate | TokenStatus::Private => {
            "other users can read the token file. Replace the token on the MCP page."
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use subspace_core::mcp_token::TokenStatus;

    use super::{Decision, Lifecycle, Listener, ServerState, Target, decide, token_reason};

    fn target(enabled: bool, port: u16) -> Target {
        Target {
            enabled,
            listener: Listener {
                address: ([127, 0, 0, 1], port).into(),
                require_token: false,
                token_revision: 0,
            },
            groups: BTreeMap::new(),
            boards: vec!["github"],
            repos: Vec::new(),
        }
    }

    fn off() -> Lifecycle {
        Lifecycle::Settled(ServerState::Off)
    }

    fn listening() -> Lifecycle {
        Lifecycle::Settled(ServerState::Listening(([127, 0, 0, 1], 7331).into()))
    }

    fn error() -> Lifecycle {
        Lifecycle::Settled(ServerState::Error("in use".into()))
    }

    fn starting(shown: ServerState) -> Lifecycle {
        Lifecycle::Starting {
            generation: 1,
            shown,
        }
    }

    #[test]
    fn disabled_stops_a_server_and_leaves_off_alone() {
        let disabled = target(false, 7331);
        assert_eq!(
            decide(&off(), None, &disabled, TokenStatus::Missing),
            Decision::Noop
        );
        let applied = target(true, 7331);
        assert_eq!(
            decide(
                &listening(),
                Some(&applied),
                &disabled,
                TokenStatus::Missing
            ),
            Decision::Stop
        );
        assert_eq!(
            decide(&error(), None, &disabled, TokenStatus::Missing),
            Decision::Stop
        );
    }

    #[test]
    fn a_disable_stops_a_pending_start_and_otherwise_it_decides_as_shown() {
        let applied = target(true, 7331);
        let disabled = target(false, 7331);
        let mut boards = applied.clone();
        boards.boards.push("irc");
        for shown in [
            ServerState::Off,
            ServerState::Error("in use".into()),
            ServerState::Listening(([127, 0, 0, 1], 7331).into()),
        ] {
            let pending = starting(shown.clone());
            let settled = Lifecycle::Settled(shown);
            assert_eq!(
                decide(&pending, Some(&applied), &disabled, TokenStatus::Missing),
                Decision::Stop
            );
            for desired in [&applied, &boards, &target(true, 7400)] {
                assert_eq!(
                    decide(&pending, Some(&applied), desired, TokenStatus::Missing),
                    decide(&settled, Some(&applied), desired, TokenStatus::Missing)
                );
            }
        }
    }

    #[test]
    fn enabling_starts_and_an_error_is_retried_without_changes() {
        let on = target(true, 7331);
        assert_eq!(
            decide(&off(), None, &on, TokenStatus::Missing),
            Decision::Start
        );
        assert_eq!(
            decide(&error(), Some(&on), &on, TokenStatus::Missing),
            Decision::Start
        );
    }

    #[test]
    fn listener_changes_restart_and_group_or_board_changes_only_swap_the_catalog() {
        let applied = target(true, 7331);
        assert_eq!(
            decide(&listening(), Some(&applied), &applied, TokenStatus::Missing),
            Decision::Noop
        );
        assert_eq!(
            decide(
                &listening(),
                Some(&applied),
                &target(true, 7400),
                TokenStatus::Missing
            ),
            Decision::Restart
        );
        let mut new_token = applied.clone();
        new_token.listener.token_revision = 1;
        assert_eq!(
            decide(
                &listening(),
                Some(&applied),
                &new_token,
                TokenStatus::Missing
            ),
            Decision::Restart
        );
        let mut groups = applied.clone();
        groups.groups.insert("github".into(), false);
        assert_eq!(
            decide(&listening(), Some(&applied), &groups, TokenStatus::Missing),
            Decision::SwapCatalog
        );
        let mut boards = applied.clone();
        boards.boards.push("irc");
        assert_eq!(
            decide(&listening(), Some(&applied), &boards, TokenStatus::Missing),
            Decision::SwapCatalog
        );
        let mut repos = applied.clone();
        repos.repos.push("acme/web".into());
        assert_eq!(
            decide(&listening(), Some(&applied), &repos, TokenStatus::Missing),
            Decision::SwapCatalog,
            "new favourite repositories rescope the served tools"
        );
        assert_eq!(
            decide(&listening(), None, &applied, TokenStatus::Missing),
            Decision::Restart
        );
    }

    #[test]
    fn a_required_token_must_exist_and_be_private() {
        let mut on = target(true, 7331);
        on.listener.require_token = true;
        for status in [TokenStatus::Missing, TokenStatus::NotPrivate] {
            assert_eq!(
                decide(&listening(), Some(&on), &on, status),
                Decision::Refuse(status)
            );
        }
        assert_eq!(
            decide(&off(), None, &on, TokenStatus::Private),
            Decision::Start
        );
        assert!(token_reason(TokenStatus::Missing).contains("missing"));
        assert!(token_reason(TokenStatus::NotPrivate).contains("other users"));
    }
}
