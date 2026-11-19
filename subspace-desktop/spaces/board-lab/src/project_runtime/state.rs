use super::{
    Completion, SessionStatus,
    diagnostics::{Diagnostics, redact},
};
use crate::project::{
    HostEvent, HostMessage, InputValue, LiveState, MessageContext, ProjectDefinition,
    ProjectRevision, WorkerEvent, WorkerMessage,
};
use anyhow::{Context, Result, ensure};
use std::collections::{BTreeMap, VecDeque};

mod completion;

fn work_capacity(status: SessionStatus, pending: usize, sequence: u64) -> Result<()> {
    ensure!(status == SessionStatus::Running, "session is not running");
    ensure!(
        pending < super::MAX_PENDING,
        "session pending work capacity is full"
    );
    ensure!(sequence < u64::MAX, "request sequence exhausted");
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PendingKind {
    Init,
    Input,
    Action,
}

struct Pending {
    kind: PendingKind,
    revision: u64,
}

/// Sole accepted LiveState and correlation owner. The supervisor supplies pipes,
/// not a second state model. Pending and untaken action outcomes share 64 slots.
pub(super) struct RuntimeState {
    live: LiveState,
    status: SessionStatus,
    next_request: u64,
    latest: Option<MessageContext>,
    pending: BTreeMap<String, Pending>,
    completions: VecDeque<Completion>,
    diagnostics: Diagnostics,
    selected: Vec<String>,
    revision: ProjectRevision,
    changes: Option<tokio::sync::watch::Sender<()>>,
}

impl RuntimeState {
    pub(super) fn new(
        definition: ProjectDefinition,
        session: &str,
        changes: Option<tokio::sync::watch::Sender<()>>,
    ) -> Result<Self> {
        Ok(Self {
            live: LiveState::new(definition, session)?,
            status: SessionStatus::Starting,
            next_request: 0,
            latest: None,
            pending: BTreeMap::new(),
            completions: VecDeque::new(),
            diagnostics: Diagnostics::default(),
            selected: Vec::new(),
            revision: ProjectRevision::new("", ""),
            changes,
        })
    }

    pub(super) fn live(&self) -> &LiveState {
        &self.live
    }

    pub(super) fn status(&self) -> SessionStatus {
        self.status
    }

    pub(super) fn configure(&mut self, revision: ProjectRevision, selected: Vec<String>) {
        self.revision = revision;
        self.selected = selected;
    }

    pub(super) fn pending(&self) -> usize {
        self.pending.len()
    }

    pub(super) fn seed_values(&mut self, values: BTreeMap<String, InputValue>) -> Result<()> {
        ensure!(self.latest.is_none(), "cannot seed an initialized session");
        let patch = crate::project::LivePatch {
            values,
            ..crate::project::LivePatch::default()
        };
        let session = self.live.session_id().to_owned();
        self.live.apply(&session, 0, patch)
    }

    pub(super) fn init(&mut self) -> Result<HostMessage> {
        ensure!(self.latest.is_none(), "session already initialized");
        self.queue(
            HostEvent::Init {
                definition: Box::new(self.live.definition().clone()),
                definition_revision: self.revision.definition.clone(),
                source_revision: self.revision.source.clone(),
                values: self.live.values().clone(),
            },
            PendingKind::Init,
        )
    }

    pub(super) fn accept(&mut self, message: WorkerMessage) -> Result<()> {
        let result = self.accept_message(message);
        // Even rejected stale completions can retain an action outcome.
        self.notify();
        result
    }

    fn accept_message(&mut self, message: WorkerMessage) -> Result<()> {
        ensure!(
            matches!(
                self.status,
                SessionStatus::Starting | SessionStatus::Running
            ),
            "session is stopped"
        );
        message.context.validate()?;
        ensure!(
            message.context.session_id == self.live.session_id(),
            "stale runtime session"
        );
        if message.context.input_revision != self.live.input_revision() {
            self.stale_completion(&message);
            anyhow::bail!("stale runtime input revision");
        }
        match message.event {
            WorkerEvent::Patch { patch } => {
                ensure!(
                    self.current_context(&message.context),
                    "unknown or stale request id"
                );
                self.live.apply(
                    &message.context.session_id,
                    message.context.input_revision,
                    patch,
                )
            }
            event => self.complete(message.context, event),
        }
    }

    pub(super) fn set_value(
        &mut self,
        key: &str,
        value: InputValue,
        expected: u64,
    ) -> Result<HostMessage> {
        self.capacity()?;
        self.live.set_value(key, value.clone(), expected)?;
        self.queue(
            HostEvent::Input {
                key: key.into(),
                value,
            },
            PendingKind::Input,
        )
    }

    pub(super) fn invoke(&mut self, action: &str, expected: u64) -> Result<HostMessage> {
        self.capacity()?;
        ensure!(
            expected == self.live.input_revision(),
            "input revision conflict"
        );
        self.live.validate_action(action)?;
        self.queue(
            HostEvent::Action {
                action: action.into(),
                values: self.live.values().clone(),
            },
            PendingKind::Action,
        )
    }

    pub(super) fn take_completions(&mut self) -> Vec<Completion> {
        self.completions.drain(..).collect()
    }

    pub(super) fn fail(&mut self, diagnostic: &str) {
        self.terminate(SessionStatus::Failed, diagnostic);
        self.notify();
    }

    pub(super) fn stop(&mut self) {
        if self.status != SessionStatus::Failed {
            self.terminate(SessionStatus::Stopped, "session stopped");
            self.notify();
        }
    }

    fn terminate(&mut self, status: SessionStatus, diagnostic: &str) {
        let message = redact(diagnostic, &self.selected);
        self.diagnostics.append(message.as_bytes(), &self.selected);
        self.status = status;
        for (request_id, pending) in std::mem::take(&mut self.pending) {
            if pending.kind == PendingKind::Action {
                self.completions.push_back(Completion {
                    request_id,
                    error: Some(message.clone()),
                });
            }
        }
    }

    pub(super) fn stderr(&mut self, bytes: &[u8]) {
        self.diagnostics.append(bytes, &self.selected);
        self.notify();
    }

    fn notify(&self) {
        if let Some(changes) = &self.changes {
            changes.send_replace(());
        }
    }

    pub(super) fn diagnostics(&self) -> &str {
        self.diagnostics.text()
    }

    fn capacity(&self) -> Result<()> {
        work_capacity(self.status, self.work_count(), self.next_request)
    }

    fn work_count(&self) -> usize {
        self.pending.len() + self.completions.len()
    }

    fn queue(&mut self, event: HostEvent, kind: PendingKind) -> Result<HostMessage> {
        self.next_request = self
            .next_request
            .checked_add(1)
            .context("request sequence exhausted")?;
        let context = MessageContext::new(
            format!("request-{}", self.next_request),
            self.live.session_id(),
            self.live.input_revision(),
        );
        let message = HostMessage::new(context.clone(), event);
        message.to_line()?;
        self.pending.insert(
            context.request_id.clone(),
            Pending {
                kind,
                revision: context.input_revision,
            },
        );
        self.latest = Some(context);
        Ok(message)
    }

    fn current_context(&self, context: &MessageContext) -> bool {
        self.latest.as_ref() == Some(context)
            || self
                .pending
                .get(&context.request_id)
                .is_some_and(|pending| pending.revision == context.input_revision)
    }
}
