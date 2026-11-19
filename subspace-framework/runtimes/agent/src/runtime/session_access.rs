use super::{PermissionMode, PermissionsSetModeRequest, Session};

/// Why a session's access could not be confirmed, and whether the reason
/// was a closed connection to the runtime rather than a refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AccessError {
    pub(crate) message: String,
    pub(crate) transport: bool,
}

impl AccessError {
    /// An SDK call failed; `message` says which check could not run.
    pub(crate) fn sdk(error: &github_copilot_sdk::Error, message: &str) -> Self {
        Self {
            message: message.to_owned(),
            transport: error.is_transport_failure(),
        }
    }

    fn refused(message: &str) -> Self {
        Self {
            message: message.to_owned(),
            transport: false,
        }
    }
}

/// Confirms the enterprise policy and the sandbox enforcement.
pub(crate) async fn check_managed_policy(session: &Session) -> Result<(), AccessError> {
    session
        .rpc()
        .managed_settings()
        .get()
        .await
        .map_err(|error| {
            AccessError::sdk(
                &error,
                "Enterprise policy could not be verified. Chat is blocked.",
            )
        })?;
    let status = session
        .rpc()
        .sandbox()
        .get_enforcement_status()
        .await
        .map_err(|error| {
            AccessError::sdk(
                &error,
                "Sandbox enforcement could not be verified. Chat is blocked.",
            )
        })?;
    if status.blocked {
        return Err(AccessError::refused(
            "Copilot sandbox enforcement blocked this session. Restart after correcting the platform or policy error.",
        ));
    }
    Ok(())
}

/// Confirms the policy and that the session still has the access mode
/// Subspace set.
pub(crate) async fn check_access_mode(session: &Session, yolo: bool) -> Result<(), AccessError> {
    check_managed_policy(session).await?;
    let actual = session
        .rpc()
        .permissions()
        .get_mode()
        .await
        .map_err(|error| {
            AccessError::sdk(
                &error,
                "Copilot access mode cannot be verified. Chat is blocked.",
            )
        })?;
    let expected = if yolo {
        PermissionMode::AllowAll
    } else {
        PermissionMode::Manual
    };
    if actual.mode != expected {
        return Err(AccessError::refused(
            "Copilot access mode changed outside Subspace. Select an access mode again before sending.",
        ));
    }
    Ok(())
}

pub(crate) async fn verify_access_mode(session: &Session, yolo: bool) -> Result<(), String> {
    check_access_mode(session, yolo)
        .await
        .map_err(|error| error.message)
}

pub(crate) async fn set_yolo(session: &Session, enabled: bool) -> Result<(), String> {
    super::ensure_managed_policy(session).await?;
    super::configure_sandbox(session, !enabled).await?;
    let mode = if enabled {
        PermissionMode::AllowAll
    } else {
        PermissionMode::Manual
    };
    let result = session
        .rpc()
        .permissions()
        .set_mode(PermissionsSetModeRequest {
            mode: mode.clone(),
            source: None,
            assisted_approval_model: None,
        })
        .await
        .map_err(|_| "Enterprise policy or the SDK rejected the access-mode change.".to_string())?;
    if !result.success || result.mode != mode {
        return Err("Copilot did not confirm the requested access mode.".into());
    }
    verify_access_mode(session, enabled).await
}

#[cfg(test)]
mod tests {
    use github_copilot_sdk::{Error, ErrorKind};

    use super::AccessError;

    #[test]
    fn a_closed_connection_is_told_apart_from_a_refusal() {
        let closed = AccessError::sdk(&Error::with_message(ErrorKind::Io, "pipe"), "check");
        assert!(closed.transport);
        let refused = AccessError::sdk(
            &Error::with_message(ErrorKind::Rpc { code: -32603 }, "no"),
            "check",
        );
        assert!(!refused.transport);
        assert_eq!(refused.message, "check");
    }
}
