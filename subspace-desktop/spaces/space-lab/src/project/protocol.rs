use super::{InputValue, LivePatch, ProjectDefinition, limits};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const PROTOCOL_VERSION: u32 = 1;

/// Every message, including unsolicited updates, carries correlation and the
/// accepted host input revision. A worker cannot advance that revision itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct MessageContext {
    pub version: u32,
    pub request_id: String,
    pub session_id: String,
    pub input_revision: u64,
}

impl MessageContext {
    pub fn new(
        request_id: impl Into<String>,
        session_id: impl Into<String>,
        input_revision: u64,
    ) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            request_id: request_id.into(),
            session_id: session_id.into(),
            input_revision,
        }
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == PROTOCOL_VERSION,
            "unsupported protocol version"
        );
        super::validation::token(&self.request_id).context("request_id")?;
        super::validation::token(&self.session_id).context("session_id")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct HostMessage {
    pub context: MessageContext,
    pub event: HostEvent,
}

impl HostMessage {
    pub fn new(context: MessageContext, event: HostEvent) -> Self {
        Self { context, event }
    }

    pub fn to_line(&self) -> Result<Vec<u8>> {
        self.context.validate()?;
        line(self)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostEvent {
    Init {
        definition: Box<ProjectDefinition>,
        definition_revision: String,
        source_revision: String,
        values: BTreeMap<String, InputValue>,
    },
    Input {
        key: String,
        value: InputValue,
    },
    Action {
        action: String,
        values: BTreeMap<String, InputValue>,
    },
    Stop,
}

/// Ready acknowledges initialization; Ack acknowledges input or Stop. Result
/// completes an action (even with an empty patch). Patch is replaceable display
/// traffic; Error explicitly fails its request and never freshens retained data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct WorkerMessage {
    pub context: MessageContext,
    pub event: WorkerEvent,
}

impl WorkerMessage {
    pub fn new(context: MessageContext, event: WorkerEvent) -> Self {
        Self { context, event }
    }

    pub fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= limits::FRAME_BYTES,
            "worker frame exceeds 256 KiB"
        );
        let message: Self = serde_json::from_slice(bytes).context("invalid worker frame")?;
        message.context.validate()?;
        if let WorkerEvent::Error { message } = &message.event {
            ensure!(
                message.len() <= limits::TEXT_BYTES,
                "worker error exceeds diagnostic budget"
            );
        }
        Ok(message)
    }

    pub fn to_line(&self) -> Result<Vec<u8>> {
        self.context.validate()?;
        line(self)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerEvent {
    Ready,
    Ack,
    Patch { patch: LivePatch },
    Result { patch: LivePatch },
    Error { message: String },
}

fn line(message: &impl Serialize) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(message)?;
    ensure!(
        bytes.len() < limits::FRAME_BYTES,
        "protocol frame exceeds 256 KiB including newline"
    );
    bytes.push(b'\n');
    Ok(bytes)
}
