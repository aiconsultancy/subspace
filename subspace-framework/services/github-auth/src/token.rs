//! `POST /login/oauth/access_token`: its requests and what its answers mean.
//! GitHub answers OAuth errors with status 200 and an `error` field.

use serde::Deserialize;
use subspace_core::Secret;

use crate::AuthError;

/// The token endpoint's path under the OAuth base.
pub const TOKEN_PATH: &str = "/login/oauth/access_token";
/// Answers are a few hundred bytes.
pub(crate) const REPLY_LIMIT: u64 = 64 * 1024;
const REQUIRED_SCOPES: [&str; 2] = ["repo", "read:org"];
/// Broader scopes that include `read:org`.
const ORG_SCOPES: [&str; 3] = ["read:org", "write:org", "admin:org"];

/// A token and the scopes GitHub granted with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Granted {
    pub token: Secret,
    pub scopes: Vec<String>,
}

/// One answer of the token endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    Token(Granted),
    /// Device flow: the user has not entered the code yet.
    Pending,
    /// Device flow: poll less often, at the new interval when GitHub gives one.
    SlowDown(Option<u64>),
    Failed(AuthError),
}

#[derive(Deserialize)]
struct Raw {
    access_token: Option<String>,
    scope: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
    interval: Option<u64>,
}

/// What a token-endpoint body says.
pub fn parse_reply(body: &[u8]) -> Reply {
    let Ok(raw) = serde_json::from_slice::<Raw>(body) else {
        return Reply::Failed(AuthError::Service(
            "GitHub's answer was not understood.".into(),
        ));
    };
    match (raw.access_token, raw.error.as_deref()) {
        (Some(token), None) if !token.is_empty() => Reply::Token(Granted {
            token: Secret::new(token),
            scopes: scopes(raw.scope.as_deref().unwrap_or_default()),
        }),
        (_, Some("authorization_pending")) => Reply::Pending,
        (_, Some("slow_down")) => Reply::SlowDown(raw.interval),
        (_, error) => Reply::Failed(failure(error, raw.error_description)),
    }
}

/// The error an `error` code stands for.
pub fn failure(error: Option<&str>, description: Option<String>) -> AuthError {
    match error {
        Some("access_denied") => AuthError::Denied,
        Some("expired_token") => AuthError::Expired,
        Some("device_flow_disabled") => AuthError::DeviceFlowDisabled,
        Some(code) => AuthError::Service(description.unwrap_or_else(|| code.to_owned())),
        None => AuthError::Service("GitHub returned no token.".into()),
    }
}

/// The scopes in GitHub's comma- or space-separated `scope` value.
pub fn scopes(value: &str) -> Vec<String> {
    value
        .split([',', ' '])
        .map(str::trim)
        .filter(|scope| !scope.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The required scopes `granted` lacks.
pub fn missing_scopes(granted: &[String]) -> Vec<&'static str> {
    let has = |wanted: &str| granted.iter().any(|scope| scope == wanted);
    REQUIRED_SCOPES
        .into_iter()
        .filter(|&required| match required {
            "read:org" => !ORG_SCOPES.iter().any(|scope| has(scope)),
            other => !has(other),
        })
        .collect()
}

/// An error naming the missing scopes, if any.
pub fn check_scopes(granted: &[String]) -> Result<(), AuthError> {
    let missing = missing_scopes(granted);
    if missing.is_empty() {
        return Ok(());
    }
    Err(AuthError::MissingScopes(missing.join(" and ")))
}

/// Posts `form` to the token endpoint and reads the answer.
pub(crate) fn request(oauth: &str, form: &[(&str, &str)]) -> Result<Reply, AuthError> {
    let url = format!("{oauth}{TOKEN_PATH}");
    let mut response = subspace_core::http::agent()
        .post(&url)
        .header("Accept", "application/json")
        .send_form(form.iter().copied())
        .map_err(|error| AuthError::Network(error.to_string()))?;
    let body = response
        .body_mut()
        .with_config()
        .limit(REPLY_LIMIT)
        .read_to_vec()
        .map_err(|error| AuthError::Network(error.to_string()))?;
    Ok(parse_reply(&body))
}

#[cfg(test)]
mod tests {
    use super::{Granted, Reply, check_scopes, missing_scopes, parse_reply, scopes};
    use crate::AuthError;
    use subspace_core::Secret;

    #[test]
    fn a_token_answer_gives_the_token_and_its_scopes() {
        let reply = parse_reply(
            br#"{"access_token":"fixture-octo","token_type":"bearer","scope":"read:org,repo"}"#,
        );
        assert_eq!(
            reply,
            Reply::Token(Granted {
                token: Secret::new("fixture-octo"),
                scopes: vec!["read:org".into(), "repo".into()],
            })
        );
        assert!(!format!("{reply:?}").contains("fixture-octo"));
    }

    #[test]
    fn error_answers_become_plain_failures_or_polling_steps() {
        let reply = |body: &str| parse_reply(body.as_bytes());
        assert_eq!(
            reply(r#"{"error":"authorization_pending"}"#),
            Reply::Pending
        );
        assert_eq!(
            reply(r#"{"error":"slow_down","interval":10}"#),
            Reply::SlowDown(Some(10))
        );
        assert_eq!(
            reply(r#"{"error":"access_denied"}"#),
            Reply::Failed(AuthError::Denied)
        );
        assert_eq!(
            reply(r#"{"error":"expired_token"}"#),
            Reply::Failed(AuthError::Expired)
        );
        assert_eq!(
            reply(
                r#"{"error":"device_flow_disabled","error_description":"Device Flow must be explicitly enabled for this App"}"#
            ),
            Reply::Failed(AuthError::DeviceFlowDisabled)
        );
        assert_eq!(
            reply(
                r#"{"error":"bad_verification_code","error_description":"The code passed is incorrect or expired."}"#
            ),
            Reply::Failed(AuthError::Service(
                "The code passed is incorrect or expired.".into()
            ))
        );
        assert!(matches!(
            reply("<html>"),
            Reply::Failed(AuthError::Service(_))
        ));
        assert!(matches!(
            reply(r#"{"access_token":""}"#),
            Reply::Failed(AuthError::Service(_))
        ));
    }

    #[test]
    fn both_scopes_are_required_and_broader_org_scopes_count() {
        assert_eq!(scopes("repo, read:org"), ["repo", "read:org"]);
        assert!(check_scopes(&scopes("repo,read:org")).is_ok());
        assert!(check_scopes(&scopes("repo,admin:org,gist")).is_ok());
        assert_eq!(missing_scopes(&scopes("read:org")), ["repo"]);
        assert_eq!(missing_scopes(&scopes("")), ["repo", "read:org"]);
        let error = check_scopes(&scopes("public_repo")).unwrap_err();
        assert_eq!(
            error.to_string(),
            "GitHub did not grant repo and read:org. Sign in again and approve every permission."
        );
    }
}
