use super::{Completion, PendingKind, RuntimeState, SessionStatus};
use crate::project::{MessageContext, WorkerEvent, WorkerMessage};
use crate::project_runtime::diagnostics::redact;
use anyhow::{Context, Result, ensure};

impl RuntimeState {
    pub(super) fn stale_completion(&mut self, message: &WorkerMessage) {
        if matches!(message.event, WorkerEvent::Patch { .. }) {
            return;
        }
        if !self
            .pending
            .get(&message.context.request_id)
            .is_some_and(|pending| pending.revision == message.context.input_revision)
        {
            return;
        }
        if let Some(pending) = self.pending.remove(&message.context.request_id)
            && pending.kind == PendingKind::Action
        {
            self.completions.push_back(Completion {
                request_id: message.context.request_id.clone(),
                error: Some("stale runtime input revision".into()),
            });
        }
    }

    pub(super) fn complete(&mut self, context: MessageContext, event: WorkerEvent) -> Result<()> {
        if !self.pending.contains_key(&context.request_id)
            && self.latest.as_ref() == Some(&context)
            && let WorkerEvent::Error { message } = &event
        {
            anyhow::bail!("{}", redact(message, &self.selected));
        }
        let pending = self
            .pending
            .get(&context.request_id)
            .context("unknown or completed request id")?;
        ensure!(
            pending.revision == context.input_revision,
            "request input revision mismatch"
        );
        let expected = completion_kind(&event, pending.kind)?;
        ensure!(pending.kind == expected, "incompatible request completion");
        let outcome = self.apply_completion(&context, event);
        self.pending.remove(&context.request_id);
        if expected == PendingKind::Action {
            self.completions.push_back(Completion {
                request_id: context.request_id,
                error: outcome
                    .as_ref()
                    .err()
                    .map(|error| redact(&error.to_string(), &self.selected)),
            });
        }
        if expected == PendingKind::Init && outcome.is_err() {
            self.fail("worker initialization failed");
        }
        outcome
    }

    fn apply_completion(&mut self, context: &MessageContext, event: WorkerEvent) -> Result<()> {
        match event {
            WorkerEvent::Ready => {
                self.status = SessionStatus::Running;
                Ok(())
            }
            WorkerEvent::Ack => Ok(()),
            WorkerEvent::Result { patch } => {
                self.live
                    .apply(&context.session_id, context.input_revision, patch)
            }
            WorkerEvent::Error { message } => anyhow::bail!("{}", redact(&message, &self.selected)),
            WorkerEvent::Patch { .. } => anyhow::bail!("unexpected patch completion"),
        }
    }
}

fn completion_kind(event: &WorkerEvent, pending: PendingKind) -> Result<PendingKind> {
    match event {
        WorkerEvent::Ready => Ok(PendingKind::Init),
        WorkerEvent::Ack => Ok(PendingKind::Input),
        WorkerEvent::Result { .. } => Ok(PendingKind::Action),
        WorkerEvent::Error { .. } => Ok(pending),
        WorkerEvent::Patch { .. } => anyhow::bail!("unexpected patch completion"),
    }
}
