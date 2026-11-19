//! What kind of failure ended a turn, and how a `session.error` maps to one.

use super::NO_ERROR_TYPE;

const UNAUTHORIZED: i32 = 401;
const PAYMENT_REQUIRED: i32 = 402;
const FORBIDDEN: i32 = 403;
const PROXY_AUTHENTICATION: i32 = 407;
const TOO_MANY_REQUESTS: i32 = 429;
const BAD_GATEWAY: i32 = 502;
const UNAVAILABLE: i32 = 503;
const GATEWAY_TIMEOUT: i32 = 504;

/// What kind of failure ended a turn, which decides what the chat tells the
/// user to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureCategory {
    NoResponse,
    SignIn,
    Limit,
    NotAllowed,
    ContextFull,
    Network,
    Stopped,
    NotResponding,
    Unknown,
}

impl FailureCategory {
    pub const ALL: [Self; 9] = [
        Self::NoResponse,
        Self::SignIn,
        Self::Limit,
        Self::NotAllowed,
        Self::ContextFull,
        Self::Network,
        Self::Stopped,
        Self::NotResponding,
        Self::Unknown,
    ];

    /// One line that says what happened.
    pub fn title(self) -> &'static str {
        match self {
            Self::NoResponse => "Copilot returned no reply",
            Self::SignIn => "Copilot did not accept the sign-in",
            Self::Limit => "Copilot rate limit or quota reached",
            Self::NotAllowed => "A Copilot policy blocked this request",
            Self::ContextFull => "The conversation is too long for the model",
            Self::Network => "Copilot could not reach its service",
            Self::Stopped => "The Copilot runtime stopped",
            Self::NotResponding => "The Copilot runtime is not responding",
            Self::Unknown => "Copilot could not finish this turn",
        }
    }

    /// One line that says what to do next.
    pub fn action(self) -> &'static str {
        match self {
            Self::NoResponse => {
                "Send the message again. If it happens again, pick another model or a lower reasoning effort."
            }
            Self::SignIn => {
                "Sign in to GitHub again, or use an account with Copilot access, then retry."
            }
            Self::Limit => "Wait for the limit to reset or pick another model, then retry.",
            Self::NotAllowed => "Pick another model, or ask your Copilot administrator for access.",
            Self::ContextFull => "Start a new chat, or pick a model with a larger context.",
            Self::Network => "Check the network and proxy settings, then retry.",
            Self::Stopped | Self::NotResponding => "Reconnect to start Copilot again, then retry.",
            Self::Unknown => "Retry. If it happens again, copy the details into a report.",
        }
    }

    /// A stable lower-case name, for logs and the debug fixture.
    pub fn slug(self) -> &'static str {
        match self {
            Self::NoResponse => "no-response",
            Self::SignIn => "sign-in",
            Self::Limit => "limit",
            Self::NotAllowed => "not-allowed",
            Self::ContextFull => "context-full",
            Self::Network => "network",
            Self::Stopped => "stopped",
            Self::NotResponding => "not-responding",
            Self::Unknown => "unknown",
        }
    }

    /// The category a [`slug`](Self::slug) names.
    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|category| category.slug() == slug.trim())
    }

    /// The error type of a failure Subspace detected itself.
    pub(super) fn runtime_error_type(self) -> &'static str {
        match self {
            Self::NotResponding => "runtime_not_responding",
            Self::Stopped => "runtime_stopped",
            _ => NO_ERROR_TYPE,
        }
    }

    /// Whether only a new runtime can recover, so the chat offers Reconnect.
    pub fn needs_reconnect(self) -> bool {
        matches!(self, Self::Stopped | Self::NotResponding)
    }
}

/// The category of a `session.error`, from the fields the SDK sends: the
/// error type first, then the remediation, then the HTTP status.
pub fn categorize(
    error_type: &str,
    status_code: Option<i32>,
    remediation: Option<&str>,
) -> FailureCategory {
    match error_type {
        "authentication" | "authorization" => return FailureCategory::SignIn,
        "rate_limit" | "quota" => return FailureCategory::Limit,
        "policy" => return FailureCategory::NotAllowed,
        "context_limit" => return FailureCategory::ContextFull,
        _ => {}
    }
    if matches!(
        remediation,
        Some("sign_in" | "switch_account" | "show_account")
    ) {
        return FailureCategory::SignIn;
    }
    match status_code {
        Some(UNAUTHORIZED | FORBIDDEN) => FailureCategory::SignIn,
        Some(PAYMENT_REQUIRED | TOO_MANY_REQUESTS) => FailureCategory::Limit,
        Some(PROXY_AUTHENTICATION | BAD_GATEWAY | UNAVAILABLE | GATEWAY_TIMEOUT) => {
            FailureCategory::Network
        }
        None if error_type == "query" => FailureCategory::NoResponse,
        _ => FailureCategory::Unknown,
    }
}
