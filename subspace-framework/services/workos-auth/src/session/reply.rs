//! What a WorkOS authentication reply becomes: the stored session and the
//! in-memory access token.

use super::{Access, EXPIRY_SKEW, FALLBACK_LIFETIME, Session};
use crate::http::Reply;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use subspace_account::{Account, Error};

/// Renewal must preserve the signed-in user and organisation.
pub(super) fn granted(
    reply: Reply,
    previous: Option<&Account>,
) -> Result<(Session, Access), Error> {
    let account = account(&reply);
    if previous
        .is_some_and(|previous| previous.id != account.id || previous.tenant != account.tenant)
    {
        return Err(Error::InvalidReply);
    }
    let lifetime = lifetime(reply.access_token.expose());
    let expires_at = Instant::now()
        .checked_add(lifetime.saturating_sub(EXPIRY_SKEW))
        .ok_or(Error::InvalidReply)?;
    Ok((
        Session {
            account,
            refresh: reply.refresh_token,
        },
        Access {
            token: reply.access_token,
            expires_at,
            wall_deadline: SystemTime::now() + lifetime.saturating_sub(EXPIRY_SKEW),
        },
    ))
}

fn account(reply: &Reply) -> Account {
    let name = [&reply.user.first_name, &reply.user.last_name]
        .into_iter()
        .flatten()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    Account {
        tenant: reply.organization_id.clone().unwrap_or_default(),
        id: reply.user.id.clone(),
        name: if name.is_empty() {
            reply.user.email.clone()
        } else {
            name
        },
        username: reply.user.email.clone(),
    }
}

/// The access token's remaining lifetime from its unverified `exp` claim.
/// The API verifies the token; this only schedules renewal.
pub(super) fn lifetime(token: &str) -> Duration {
    #[derive(Deserialize)]
    struct Claims {
        exp: u64,
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    token
        .split('.')
        .nth(1)
        .and_then(|payload| URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok())
        .and_then(|bytes| serde_json::from_slice::<Claims>(&bytes).ok())
        .map_or(FALLBACK_LIFETIME, |claims| {
            Duration::from_secs(claims.exp.saturating_sub(now))
        })
}

pub(super) fn remaining(access: &Access) -> Duration {
    access
        .expires_at
        .saturating_duration_since(Instant::now())
        .min(
            access
                .wall_deadline
                .duration_since(SystemTime::now())
                .unwrap_or_default(),
        )
}
