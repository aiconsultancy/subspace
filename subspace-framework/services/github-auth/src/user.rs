//! The REST calls around a sign-in: who owns a token, and revoking it.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::Deserialize;
use subspace_core::Secret;
use subspace_core::credentials::is_login;
use subspace_core::http::status;

use crate::{AuthError, Endpoints};

const USER_LIMIT: u64 = 64 * 1024;
const ACCEPT: &str = "application/vnd.github+json";

/// The login that owns `token`, from `GET /user`.
pub(crate) fn login(endpoints: &Endpoints, token: &Secret) -> Result<String, AuthError> {
    #[derive(Deserialize)]
    struct User {
        login: String,
    }
    let failed = |error: String| AuthError::Network(error);
    let mut response = subspace_core::http::agent()
        .get(format!("{}/user", endpoints.api))
        .header("Accept", ACCEPT)
        .header("Authorization", format!("Bearer {}", token.expose()))
        .call()
        .map_err(|error| failed(error.to_string()))?;
    if !status::is_success(response.status().as_u16()) {
        return Err(AuthError::Service(format!(
            "GitHub did not accept the new token (HTTP {}).",
            response.status().as_u16()
        )));
    }
    let body = response
        .body_mut()
        .with_config()
        .limit(USER_LIMIT)
        .read_to_vec()
        .map_err(|error| failed(error.to_string()))?;
    let user: User = serde_json::from_slice(&body)
        .map_err(|_| AuthError::Service("GitHub did not name the account.".into()))?;
    is_login(&user.login)
        .then_some(user.login)
        .ok_or_else(|| AuthError::Service("GitHub named an account Subspace cannot use.".into()))
}

/// Best effort: `DELETE /applications/{client_id}/grant`, which also revokes
/// every token of the grant. Failures are ignored; sign-out has already
/// forgotten the token.
pub(crate) fn revoke(endpoints: &Endpoints, client: (&str, &Secret), token: &Secret) {
    let (id, secret) = client;
    let basic = STANDARD.encode(format!("{id}:{}", secret.expose()));
    let body = serde_json::json!({ "access_token": token.expose() });
    let _ = subspace_core::http::agent()
        .delete(format!("{}/applications/{id}/grant", endpoints.api))
        .header("Accept", ACCEPT)
        .header("Authorization", format!("Basic {basic}"))
        .header("Content-Type", "application/json")
        .force_send_body()
        .send(body.to_string());
}
