use super::SESSION_ID_LIMIT;
use github_copilot_sdk::types::SessionId;

/// A Copilot session identifier Subspace will act on. Anything else is rejected
/// rather than passed to the runtime.
pub(super) fn parse_session_id(id: &str) -> Result<SessionId, String> {
    if id.is_empty()
        || id.len() > SESSION_ID_LIMIT
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("Invalid Copilot session identifier.".into());
    }
    Ok(SessionId::new(id.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::parse_session_id;

    #[test]
    fn only_well_formed_identifiers_reach_the_runtime() {
        assert_eq!(parse_session_id("abc-123").unwrap().as_str(), "abc-123");
        assert!(parse_session_id("").is_err());
        assert!(parse_session_id("../other-account").is_err());
        assert!(parse_session_id(&"a".repeat(super::SESSION_ID_LIMIT + 1)).is_err());
    }
}
