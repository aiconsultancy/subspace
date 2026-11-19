//! The device flow (RFC 8628): GitHub gives a code for the user to enter at
//! `github.com/login/device`, and Subspace polls the token endpoint until
//! the user approves, declines or the code expires. It needs no client
//! secret, so builds without one use it.

use std::time::{Duration, Instant};

use serde::Deserialize;
use subspace_core::Secret;

use crate::token::{self, Granted, Reply};
use crate::{AuthError, Cancel, Endpoints, Pending, Prompt, SignedIn, Started};

pub const DEVICE_CODE_PATH: &str = "/login/device/code";
pub const GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";
/// RFC 8628's default polling interval, in seconds.
const DEFAULT_INTERVAL: u64 = 5;
/// How much `slow_down` adds to the interval, in seconds.
const SLOW_DOWN_STEP: u64 = 5;
/// GitHub's device codes last 15 minutes.
const DEFAULT_EXPIRY: u64 = 15 * 60;

/// GitHub's answer to a device-code request.
#[derive(Debug, PartialEq, Eq)]
pub struct DeviceCode {
    pub device_code: Secret,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Deserialize)]
struct Raw {
    device_code: Option<String>,
    user_code: Option<String>,
    verification_uri: Option<String>,
    expires_in: Option<u64>,
    interval: Option<u64>,
    error: Option<String>,
    error_description: Option<String>,
}

/// Reads a device-code answer.
pub fn parse_code(body: &[u8]) -> Result<DeviceCode, AuthError> {
    let raw: Raw = serde_json::from_slice(body)
        .map_err(|_| AuthError::Service("GitHub's answer was not understood.".into()))?;
    if raw.error.is_some() {
        return Err(token::failure(raw.error.as_deref(), raw.error_description));
    }
    let (Some(device_code), Some(user_code), Some(verification_uri)) =
        (raw.device_code, raw.user_code, raw.verification_uri)
    else {
        return Err(token::failure(None, None));
    };
    Ok(DeviceCode {
        device_code: Secret::new(device_code),
        user_code,
        verification_uri,
        expires_in: raw.expires_in.unwrap_or(DEFAULT_EXPIRY),
        interval: raw.interval.unwrap_or(DEFAULT_INTERVAL),
    })
}

/// What to do after one poll.
#[derive(Debug, PartialEq, Eq)]
pub enum Poll {
    /// Wait `interval` seconds and poll again.
    Again {
        interval: u64,
    },
    Done(Result<Granted, AuthError>),
}

/// The next step after `reply`, polling every `interval` seconds so far.
pub fn after(reply: Reply, interval: u64) -> Poll {
    match reply {
        Reply::Token(granted) => Poll::Done(Ok(granted)),
        Reply::Pending => Poll::Again { interval },
        Reply::SlowDown(next) => Poll::Again {
            interval: next.unwrap_or(0).max(interval + SLOW_DOWN_STEP),
        },
        Reply::Failed(error) => Poll::Done(Err(error)),
    }
}

/// Asks GitHub for a device code.
pub(crate) fn start(
    client_id: &str,
    endpoints: &Endpoints,
    time_limit: Duration,
) -> Result<Started, AuthError> {
    let code = request_code(client_id, endpoints)?;
    let prompt = Prompt::Device {
        user_code: code.user_code.clone(),
        verification_uri: code.verification_uri.clone(),
    };
    let started = Instant::now();
    let pending = DevicePending {
        endpoints: endpoints.clone(),
        client_id: client_id.to_owned(),
        expires: started + Duration::from_secs(code.expires_in),
        limit: started + time_limit,
        code,
    };
    Ok(Started {
        prompt,
        pending: Box::new(pending),
    })
}

fn request_code(client_id: &str, endpoints: &Endpoints) -> Result<DeviceCode, AuthError> {
    let network = |error: ureq::Error| AuthError::Network(error.to_string());
    let mut response = subspace_core::http::agent()
        .post(format!("{}{DEVICE_CODE_PATH}", endpoints.oauth))
        .header("Accept", "application/json")
        .send_form([("client_id", client_id), ("scope", crate::config::SCOPES)])
        .map_err(network)?;
    let body = response
        .body_mut()
        .with_config()
        .limit(token::REPLY_LIMIT)
        .read_to_vec()
        .map_err(network)?;
    parse_code(&body)
}

struct DevicePending {
    endpoints: Endpoints,
    client_id: String,
    code: DeviceCode,
    expires: Instant,
    limit: Instant,
}

impl DevicePending {
    /// The error once the code expired or the time limit passed.
    fn overdue(&self) -> Option<AuthError> {
        let now = Instant::now();
        if now >= self.expires {
            return Some(AuthError::Expired);
        }
        (now >= self.limit).then_some(AuthError::TimedOut)
    }

    fn poll(&self) -> Result<Reply, AuthError> {
        let form = [
            ("client_id", self.client_id.as_str()),
            ("device_code", self.code.device_code.expose()),
            ("grant_type", GRANT_TYPE),
        ];
        token::request(&self.endpoints.oauth, &form)
    }
}

impl Pending for DevicePending {
    fn finish(self: Box<Self>, cancel: &Cancel) -> Result<SignedIn, AuthError> {
        let mut interval = self.code.interval;
        loop {
            crate::sleep(Duration::from_secs(interval), cancel)?;
            if let Some(error) = self.overdue() {
                return Err(error);
            }
            match after(self.poll()?, interval) {
                Poll::Again { interval: next } => interval = next,
                Poll::Done(granted) => return crate::signed_in(&self.endpoints, granted?),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Poll, after, parse_code};
    use crate::AuthError;
    use crate::token::{Granted, Reply};
    use subspace_core::Secret;

    #[test]
    fn a_device_code_answer_gives_the_code_to_show_and_the_polling_interval() {
        let code = parse_code(
            br#"{"device_code":"secret-device","user_code":"WDJB-MJHT","verification_uri":"https://github.com/login/device","expires_in":899,"interval":5}"#,
        )
        .unwrap();
        assert_eq!(code.user_code, "WDJB-MJHT");
        assert_eq!(code.verification_uri, "https://github.com/login/device");
        assert_eq!((code.expires_in, code.interval), (899, 5));
        assert!(!format!("{code:?}").contains("secret-device"));
    }

    #[test]
    fn a_disabled_device_flow_is_a_clear_error() {
        let error = parse_code(br#"{"error":"device_flow_disabled","error_description":"Device Flow must be explicitly enabled for this App","error_uri":"https://docs.github.com"}"#)
            .unwrap_err();
        assert_eq!(error, AuthError::DeviceFlowDisabled);
        assert!(error.to_string().contains("gh auth login"));
        assert!(matches!(parse_code(b"{}"), Err(AuthError::Service(_))));
    }

    #[test]
    fn polling_waits_slows_down_and_stops_as_github_says() {
        assert_eq!(after(Reply::Pending, 5), Poll::Again { interval: 5 });
        assert_eq!(
            after(Reply::SlowDown(None), 5),
            Poll::Again { interval: 10 }
        );
        assert_eq!(
            after(Reply::SlowDown(Some(15)), 5),
            Poll::Again { interval: 15 }
        );
        assert_eq!(
            after(Reply::SlowDown(Some(1)), 5),
            Poll::Again { interval: 10 }
        );
        assert_eq!(
            after(Reply::Failed(AuthError::Denied), 5),
            Poll::Done(Err(AuthError::Denied))
        );
        assert_eq!(
            after(Reply::Failed(AuthError::Expired), 5),
            Poll::Done(Err(AuthError::Expired))
        );
        let granted = Granted {
            token: Secret::new("token"),
            scopes: Vec::new(),
        };
        assert_eq!(
            after(Reply::Token(granted.clone()), 5),
            Poll::Done(Ok(granted))
        );
    }
}
