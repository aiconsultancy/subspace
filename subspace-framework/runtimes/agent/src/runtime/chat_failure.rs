//! How one chat tracks its running turn and reports a failed one: a
//! `session.error`, a closed event stream, a message the runtime refused, or
//! (through the watchdog) a runtime that stopped answering. Every failure is
//! a [`TurnFailure`] with an error log record. Turns are fenced by
//! generation (see `watchdog/fence.rs`), so a turn is ended once.

use std::time::Instant;

use super::super::session::AccessError;
use super::super::watchdog::fence::{self, Activity, Late};
use super::super::{ChatEvent, ChatId, Event, EventQueue, Value, lock};
use super::{Chat, Env};
use crate::failure::{
    FailureCategory, SEND_FAILED, STREAM_CLOSED, TurnContext, TurnFailure, log_failure,
    log_turn_done, log_turn_started,
};

/// The error type of a send that failed Subspace's access check.
const ACCESS_CHECK: &str = "access_check";
/// The error type of a send the runtime refused.
const SEND_REJECTED: &str = "send_rejected";
/// What a refused send says when the SDK gave no text.
const REJECTED: &str =
    "Copilot rejected the message. Check authentication, model access and policy.";
/// What a send says when the connection closed during the access check.
const CHECK_CLOSED: &str =
    "The connection to the Copilot runtime closed while Subspace checked the access mode.";

/// Which turn a chat is on, and whether the watchdog ended it.
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::runtime) struct Generation {
    pub(in crate::runtime) current: u64,
    pub(in crate::runtime) expired: bool,
}

/// Reports a failure: masks the credentials the runtime knows, writes the
/// error record and queues the failure for the chat.
pub(in crate::runtime) async fn report(
    events: &EventQueue,
    chat: ChatId,
    mut failure: TurnFailure,
) {
    events.clean_failure(&mut failure);
    log_failure(chat, &failure);
    events
        .push(Event::of(chat, ChatEvent::Failure(Box::new(failure))))
        .await;
}

/// Reports a failure that ended the running turn `turn`, then ends it.
pub(in crate::runtime) async fn end_reported(
    events: &EventQueue,
    chat: ChatId,
    turn: &TurnContext,
    failure: TurnFailure,
) {
    let category = failure.category;
    report(events, chat, failure).await;
    log_turn_done(chat, turn, Some(category));
    events.push(Event::of(chat, ChatEvent::TurnDone)).await;
}

impl Chat {
    /// A message starts a turn with a new generation, or steers the running
    /// one and adds its attachments. Either way the silence the watchdog
    /// measures restarts.
    pub(in crate::runtime) fn begin_turn(
        &mut self,
        env: &Env<'_>,
        steering: bool,
        attachments: usize,
        submitted: Vec<String>,
    ) {
        self.busy = true;
        self.last_event = Instant::now();
        if steering {
            self.turn.steer(attachments, submitted);
        } else {
            self.fence.current += 1;
            self.fence.expired = false;
            self.turn.begin(attachments, submitted);
            log_turn_started(self.id, &self.turn);
        }
        self.publish(env);
    }

    /// The turn ended: a debug record with how it ended.
    pub(in crate::runtime) fn end_turn(&mut self, failure: Option<FailureCategory>) {
        if self.turn.sent.is_some() {
            log_turn_done(self.id, &self.turn, failure);
        }
        self.turn.finish();
    }

    /// Shares the chat's state with the watchdog.
    pub(in crate::runtime) fn publish(&self, env: &Env<'_>) {
        fence::update(
            &mut lock(&env.setup.activity),
            self.id,
            Activity {
                busy: self.busy,
                last_event: self.last_event,
                turn: self.turn.clone(),
                generation: self.fence.current,
                expired: None,
            },
        );
    }

    /// Takes over a turn the watchdog ended, so the next message starts a
    /// new turn instead of steering a dead one.
    pub(in crate::runtime) fn reconcile(&mut self, env: &Env<'_>) {
        let ended = fence::expired(&lock(&env.setup.activity), self.id, self.fence.current);
        if self.busy && ended {
            self.watchdog_ended();
        }
    }

    /// Whether to handle an SDK event of type `kind`. After the watchdog
    /// ended the turn, its own idle or error is dropped, and new output
    /// re-arms the turn as a new generation. An ending event the watchdog
    /// beat by a moment is dropped as well.
    pub(in crate::runtime) fn admit(&mut self, env: &Env<'_>, kind: &str) -> bool {
        match fence::late(self.fence.expired, kind) {
            Late::Ignore => return false,
            Late::Rearm => self.rearm(env),
            Late::Normal => {}
        }
        let ends = matches!(kind, "session.idle" | "session.error");
        !ends || self.settle(env)
    }

    /// The actor ends its turn, unless the watchdog already did.
    fn settle(&mut self, env: &Env<'_>) -> bool {
        let settled = fence::settle(&mut lock(&env.setup.activity), self.id, self.fence.current);
        if !settled {
            self.watchdog_ended();
        }
        settled
    }

    fn watchdog_ended(&mut self) {
        self.busy = false;
        self.fence.expired = true;
        self.turn.finish();
    }

    /// The runtime went on with a turn the watchdog ended.
    fn rearm(&mut self, env: &Env<'_>) {
        self.fence.current += 1;
        self.fence.expired = false;
        self.busy = true;
        self.last_event = Instant::now();
        self.turn.resume();
        self.publish(env);
    }

    /// Reports a `session.error` (`failed`), with what the chat knows about
    /// the turn, and returns its category. The chat reports it and ends the
    /// turn before [`Self::recheck_policy`], so a check that hangs cannot
    /// hold them back.
    pub(in crate::runtime) async fn session_failed(
        &self,
        env: &Env<'_>,
        failed: bool,
        data: &Value,
    ) -> Option<FailureCategory> {
        if !failed {
            return None;
        }
        let failure =
            TurnFailure::from_session_error(data, &self.turn, env.setup.runtime_version.as_deref());
        let category = failure.category;
        report(env.events, self.id, failure).await;
        Some(category)
    }

    /// After a `session.error` (`failed`), checks the policy again; a chat
    /// whose policy is not confirmed refuses to send.
    pub(in crate::runtime) async fn recheck_policy(&mut self, failed: bool) {
        if failed {
            self.verified = super::super::ensure_managed_policy(&self.session)
                .await
                .is_ok();
        }
    }

    /// The event stream closed, so the runtime is gone.
    pub(in crate::runtime) async fn stream_closed(&mut self, env: &Env<'_>) {
        let failure = self.stopped(env, STREAM_CLOSED);
        self.end_with(env, failure).await;
    }

    /// A message failed to reach Copilot. A steering message leaves the
    /// running turn to its own stream.
    pub(in crate::runtime) async fn send_failed(
        &mut self,
        env: &Env<'_>,
        steering: bool,
        failure: TurnFailure,
    ) {
        if steering {
            report(env.events, self.id, failure).await;
            return;
        }
        self.end_with(env, failure).await;
    }

    /// Why Subspace's access check refused to send: a closed connection is a
    /// stopped runtime, anything else a refusal.
    pub(in crate::runtime) fn access_failure(
        &self,
        env: &Env<'_>,
        error: &AccessError,
    ) -> TurnFailure {
        if error.transport {
            return self.stopped(env, CHECK_CLOSED);
        }
        TurnFailure::rejected(
            ACCESS_CHECK,
            &error.message,
            &self.turn,
            env.setup.runtime_version.as_deref(),
        )
    }

    /// Why `session.send` failed: a closed connection is a stopped runtime;
    /// anything else is a refusal with the SDK's text.
    pub(in crate::runtime) fn send_failure(
        &self,
        env: &Env<'_>,
        error: &github_copilot_sdk::Error,
    ) -> TurnFailure {
        if error.is_transport_failure() {
            return self.stopped(env, SEND_FAILED);
        }
        let text = error.to_string();
        let message = Some(text.as_str())
            .filter(|text| !text.trim().is_empty())
            .unwrap_or(REJECTED);
        TurnFailure::rejected(
            SEND_REJECTED,
            message,
            &self.turn,
            env.setup.runtime_version.as_deref(),
        )
    }

    /// Reports a failure, and ends the turn when one runs. A turn the
    /// watchdog already ended is not reported again.
    async fn end_with(&mut self, env: &Env<'_>, failure: TurnFailure) {
        if !self.busy {
            report(env.events, self.id, failure).await;
            return;
        }
        if !self.settle(env) {
            return;
        }
        self.busy = false;
        end_reported(env.events, self.id, &self.turn, failure).await;
        self.turn.finish();
    }

    fn stopped(&self, env: &Env<'_>, message: &str) -> TurnFailure {
        TurnFailure::stopped(
            FailureCategory::Stopped,
            message,
            &self.turn,
            env.setup.runtime_version.as_deref(),
        )
    }
}
