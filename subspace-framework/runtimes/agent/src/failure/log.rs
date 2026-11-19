//! Turn records under the `agent` log target. A record holds identifiers,
//! counts and times only: never the prompt, attachment contents or a token.

use subspace_core::log::{Level, Logger};

use super::{FailureCategory, TurnContext, TurnFailure, present};
use crate::ChatId;

const TARGET: &str = "agent";

/// A debug record when a chat sends a new turn.
pub(crate) fn log_turn_started(chat: ChatId, turn: &TurnContext) {
    write(Level::Debug, "turn started", &started_fields(chat, turn));
}

/// A debug record when a turn ends. `failure` is the category of the
/// failure that ended it, if one did.
pub(crate) fn log_turn_done(chat: ChatId, turn: &TurnContext, failure: Option<FailureCategory>) {
    write(
        Level::Debug,
        "turn done",
        &done_fields(chat, turn, outcome(failure)),
    );
}

/// How a turn ended, for the `outcome` field of "turn done".
pub(super) fn outcome(failure: Option<FailureCategory>) -> &'static str {
    match failure {
        None => "done",
        Some(FailureCategory::Stopped) => "stopped",
        Some(FailureCategory::NotResponding) => "not_responding",
        Some(_) => "failed",
    }
}

/// An error record with every present fact of a failed turn.
pub(crate) fn log_failure(chat: ChatId, failure: &TurnFailure) {
    write(Level::Error, "turn failed", &failure_fields(chat, failure));
}

fn write(level: Level, message: &str, fields: &[(&'static str, String)]) {
    let fields: Vec<(&str, &str)> = fields
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect();
    Logger::new(TARGET).record(level, message, &fields);
}

fn started_fields(chat: ChatId, turn: &TurnContext) -> Vec<(&'static str, String)> {
    present([
        ("chat", Some(chat.to_string())),
        ("model", turn.model.clone()),
        ("effort", turn.effort.clone()),
        ("attachments", Some(turn.attachments.to_string())),
    ])
}

fn done_fields(chat: ChatId, turn: &TurnContext, outcome: &str) -> Vec<(&'static str, String)> {
    present([
        ("chat", Some(chat.to_string())),
        ("outcome", Some(outcome.to_owned())),
        (
            "elapsed_ms",
            turn.sent.map(|sent| sent.elapsed().as_millis().to_string()),
        ),
        ("delta_received", Some(turn.delta_received.to_string())),
    ])
}

fn failure_fields(chat: ChatId, failure: &TurnFailure) -> Vec<(&'static str, String)> {
    present([
        ("chat", Some(chat.to_string())),
        ("category", Some(failure.category.slug().to_owned())),
        ("error_type", Some(failure.error_type.clone())),
        ("message", Some(failure.message.clone())),
        (
            "status_code",
            failure.status_code.map(|code| code.to_string()),
        ),
        ("error_code", failure.error_code.clone()),
        ("provider_call_id", failure.provider_call_id.clone()),
        ("service_request_id", failure.service_request_id.clone()),
        (
            "originating_message_id",
            failure.originating_message_id.clone(),
        ),
        ("remediation", failure.remediation.clone()),
        ("model", failure.model.clone()),
        ("effort", failure.effort.clone()),
        ("runtime_version", failure.runtime_version.clone()),
        ("sdk_version", Some(failure.sdk_version.to_owned())),
        (
            "elapsed_ms",
            failure
                .elapsed
                .map(|elapsed| elapsed.as_millis().to_string()),
        ),
        ("delta_received", Some(failure.delta_received.to_string())),
        ("attachments", Some(failure.attachments.to_string())),
    ])
}
