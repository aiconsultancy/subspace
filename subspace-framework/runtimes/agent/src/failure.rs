//! Why a Copilot turn failed, with the facts that help to report it. The
//! runtime builds a [`TurnFailure`] from a `session.error` event, or when the
//! Copilot runtime stops or stops answering, and the chat panel shows its
//! title, its action and its details.
//!
//! A failure never holds the prompt, attachment contents or a credential: the
//! message passes through the crate's token redaction, and the other fields
//! are identifiers, counts and times.

mod category;
// Copilot turn records; Pi and ACP log nothing of their own here.
#[cfg(feature = "copilot")]
mod log;
mod turn;

pub use category::{FailureCategory, categorize};

use std::time::Duration;

use chrono::{DateTime, Local};
use serde_json::Value;

#[cfg(feature = "copilot")]
pub(crate) use log::{log_failure, log_turn_done, log_turn_started};
pub use turn::TurnContext;

/// The Copilot SDK version Subspace is built with. Keep it equal to the
/// `github-copilot-sdk` pin in this crate's `Cargo.toml`; a test checks it.
pub const SDK_VERSION: &str = "1.0.34";

/// What a chat reports when its event stream closes, which happens when the
/// Copilot runtime process exits.
pub const STREAM_CLOSED: &str = "The Copilot event stream closed. The runtime process exited.";
/// What a chat reports when the runtime could not take a message.
pub const SEND_FAILED: &str =
    "The Copilot runtime could not take the message. The connection to the runtime is closed.";
/// What every running turn reports when the runtime stops answering.
pub const NOT_RESPONDING: &str =
    "The Copilot runtime sent nothing for 90 seconds and did not answer a ping within 15 seconds.";

/// What a message says when the SDK supplied none.
const NO_MESSAGE: &str = "No error details were supplied by the SDK.";
/// The error type of a `session.error` that had none.
const NO_ERROR_TYPE: &str = "runtime";
/// What replaces a submitted text in a failure.
const REMOVED: &str = "[message text removed]";
/// The shortest submitted text that is removed.
const SCRUB_MIN_CHARS: usize = 4;
/// How Details says whether a reply started, indexed by the flag.
const YES_NO: [&str; 2] = ["no", "yes"];

/// Why one turn failed, and the facts that help to report it.
#[derive(Clone, Debug, PartialEq)]
pub struct TurnFailure {
    pub category: FailureCategory,
    /// The SDK `errorType`, or `runtime_stopped` and `runtime_not_responding`.
    pub error_type: String,
    /// The SDK message, redacted.
    pub message: String,
    pub status_code: Option<i32>,
    pub error_code: Option<String>,
    /// The `x-github-request-id` of the failed call.
    pub provider_call_id: Option<String>,
    /// The `x-copilot-service-request-id` of the failed call.
    pub service_request_id: Option<String>,
    pub originating_message_id: Option<String>,
    pub remediation: Option<String>,
    pub url: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub runtime_version: Option<String>,
    pub sdk_version: &'static str,
    /// The time since the turn was sent, when a turn ran.
    pub elapsed: Option<Duration>,
    pub delta_received: bool,
    pub attachments: usize,
    pub occurred_at: DateTime<Local>,
}

/// What every failure records about its turn.
struct Basis<'a> {
    category: FailureCategory,
    error_type: String,
    message: String,
    turn: &'a TurnContext,
    runtime: Option<&'a str>,
}

impl TurnFailure {
    /// A failure from a `session.error` payload. A missing field stays
    /// absent; it is never invented.
    pub fn from_session_error(data: &Value, turn: &TurnContext, runtime: Option<&str>) -> Self {
        let error_type = text(data, "errorType").unwrap_or_else(|| NO_ERROR_TYPE.to_owned());
        let status_code = data["statusCode"]
            .as_i64()
            .and_then(|code| i32::try_from(code).ok());
        let remediation = text(data, "remediation");
        let category = categorize(&error_type, status_code, remediation.as_deref());
        let message = text(data, "message").unwrap_or_else(|| NO_MESSAGE.to_owned());
        Self {
            status_code,
            remediation,
            error_code: text(data, "errorCode"),
            provider_call_id: text(data, "providerCallId"),
            service_request_id: text(data, "serviceRequestId"),
            originating_message_id: text(data, "originatingMessageId"),
            url: text(data, "url"),
            ..Self::new(Basis {
                category,
                error_type,
                message,
                turn,
                runtime,
            })
        }
        .scrubbed(turn)
    }

    /// A failure Subspace detected itself: the runtime stopped or stopped
    /// answering.
    pub fn stopped(
        category: FailureCategory,
        message: &str,
        turn: &TurnContext,
        runtime: Option<&str>,
    ) -> Self {
        Self::new(Basis {
            category,
            error_type: category.runtime_error_type().to_owned(),
            message: clean(message),
            turn,
            runtime,
        })
        .scrubbed(turn)
    }

    /// The failure with every text the turn sent removed, so a message the
    /// runtime echoes never puts the prompt or a context item in Details or
    /// the log.
    fn scrubbed(mut self, turn: &TurnContext) -> Self {
        self.redact_with(|text| scrub(text, &turn.submitted));
        self
    }

    /// A message the runtime refused before a turn could start, such as a
    /// failed access check or a rejected send. `error_type` says which.
    pub fn rejected(
        error_type: &str,
        message: &str,
        turn: &TurnContext,
        runtime: Option<&str>,
    ) -> Self {
        Self {
            error_type: error_type.to_owned(),
            ..Self::stopped(FailureCategory::Unknown, message, turn, runtime)
        }
    }

    /// Applies `redact` to every text the failure shows or logs, such as a
    /// replacement of the credentials the runtime knows.
    pub fn redact_with(&mut self, redact: impl Fn(&str) -> String) {
        self.message = redact(&self.message);
        self.error_type = redact(&self.error_type);
        for field in [
            &mut self.error_code,
            &mut self.provider_call_id,
            &mut self.service_request_id,
            &mut self.originating_message_id,
            &mut self.remediation,
            &mut self.url,
        ]
        .into_iter()
        .flatten()
        {
            *field = redact(field);
        }
    }

    fn new(basis: Basis<'_>) -> Self {
        let Basis {
            category,
            error_type,
            message,
            turn,
            runtime,
        } = basis;
        Self {
            category,
            error_type,
            message,
            status_code: None,
            error_code: None,
            provider_call_id: None,
            service_request_id: None,
            originating_message_id: None,
            remediation: None,
            url: None,
            model: turn.model.clone(),
            effort: turn.effort.clone(),
            runtime_version: runtime.map(str::to_owned),
            sdk_version: SDK_VERSION,
            elapsed: turn.sent.map(|sent| sent.elapsed()),
            delta_received: turn.delta_received,
            attachments: turn.attachments,
            occurred_at: Local::now(),
        }
    }

    /// The labelled facts the chat shows under Details, present fields only.
    pub fn details(&self) -> Vec<(&'static str, String)> {
        present([
            ("Type", Some(self.error_type.clone())),
            ("Message", Some(self.message.clone())),
            ("Status", self.status_code.map(|code| code.to_string())),
            ("Error code", self.error_code.clone()),
            ("Request ID", self.provider_call_id.clone()),
            ("Service request ID", self.service_request_id.clone()),
            ("Message ID", self.originating_message_id.clone()),
            ("Model", self.model.clone()),
            ("Reasoning effort", self.effort.clone()),
            ("Runtime", self.runtime_version.clone()),
            ("SDK", Some(self.sdk_version.to_owned())),
            (
                "Time",
                Some(self.occurred_at.format("%Y-%m-%d %H:%M:%S %:z").to_string()),
            ),
            (
                "Elapsed",
                self.elapsed
                    .map(|elapsed| format!("{:.1} s", elapsed.as_secs_f64())),
            ),
            (
                "Reply started",
                Some(YES_NO[usize::from(self.delta_received)].to_owned()),
            ),
            ("Attachments", Some(self.attachments.to_string())),
        ])
    }

    /// The details as `Label: value` lines, for Copy details.
    pub fn details_text(&self) -> String {
        lines(&self.details())
    }
}

fn lines(rows: &[(&'static str, String)]) -> String {
    rows.iter()
        .map(|(label, value)| format!("{label}: {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The rows that have a value, in order.
fn present<const N: usize>(
    rows: [(&'static str, Option<String>); N],
) -> Vec<(&'static str, String)> {
    rows.into_iter()
        .filter_map(|(label, value)| value.map(|value| (label, value)))
        .collect()
}

/// A non-empty string field of an SDK payload, with credentials masked.
fn text(data: &Value, field: &str) -> Option<String> {
    data[field]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(clean)
}

/// `text` with each submitted text of at least [`SCRUB_MIN_CHARS`]
/// characters replaced. A text of one to three characters is kept, so short
/// fragments such as "ok" are not removed from every word they occur in.
fn scrub(text: &str, submitted: &[String]) -> String {
    submitted
        .iter()
        .map(|sent| sent.trim())
        .filter(|sent| sent.chars().count() >= SCRUB_MIN_CHARS)
        .fold(text.to_owned(), |text, sent| text.replace(sent, REMOVED))
}

/// `text` with GitHub tokens, authorization headers and other credential
/// patterns masked, by the runtime's and the logger's redactors.
pub(crate) fn clean(text: &str) -> String {
    subspace_core::log::redact(&crate::runtime::redact(text))
}

#[cfg(test)]
mod record_tests;
#[cfg(test)]
mod tests;
